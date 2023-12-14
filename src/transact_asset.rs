use frame_support::traits::Get;
use sp_runtime::DispatchError;
use sp_std::boxed::Box;
use xcm::v3::{prelude::*, Error as XcmError, Result as XcmResult};
use xcm_executor::traits::{ConvertLocation, Error as XcmExecutorError, TransactAsset};

use crate::{
    traits::{DerivativeWithdrawal, DispatchErrorToXcmError, NftInterface},
    CategorizedToken, CollectionIdOf, Config, DerivativeIdStatus, DerivativeIdToForeignInstance,
    Event, ForeignInstanceToDerivativeIdStatus, ForeignToken, LocationToAccountIdOf, NativeTokenOf,
    Pallet, Token, TokenIdOf,
};

const LOG_TARGET: &str = "xcm::xnft::transactor";

impl<T: Config<I>, I: 'static> TransactAsset for Pallet<T, I> {
    fn deposit_asset(
        asset: &MultiAsset,
        who: &MultiLocation,
        context: Option<&cumulus_primitives_core::XcmContext>,
    ) -> XcmResult {
        log::trace!(
            target: LOG_TARGET,
            "deposit_asset asset: {asset:?}, who: {who:?}, context: {context:?}",
        );

        let Fungibility::NonFungible(asset_instance) = asset.fun else {
            return Err(XcmExecutorError::AssetNotHandled.into());
        };

        let to = <LocationToAccountIdOf<T, I>>::convert_location(who)
            .ok_or(XcmExecutorError::AccountIdConversionFailed)?;

        let token = Self::asset_instance_to_token(&asset.id, &asset_instance)?;

        Self::deposit_asset_instance(token, &to)
    }

    fn withdraw_asset(
        asset: &MultiAsset,
        who: &MultiLocation,
        context: Option<&cumulus_primitives_core::XcmContext>,
    ) -> Result<xcm_executor::Assets, XcmError> {
        log::trace!(
            target: LOG_TARGET,
            "withdraw_asset asset: {asset:?}, who: {who:?}, context: {context:?}",
        );

        let Fungibility::NonFungible(asset_instance) = asset.fun else {
            return Err(XcmExecutorError::AssetNotHandled.into());
        };

        let from = <LocationToAccountIdOf<T, I>>::convert_location(who)
            .ok_or(XcmExecutorError::AccountIdConversionFailed)?;

        let token = Self::asset_instance_to_token(&asset.id, &asset_instance)?;

        Self::withdraw_asset_instance(token, &from).map(|()| asset.clone().into())
    }

    fn transfer_asset(
        asset: &MultiAsset,
        from: &MultiLocation,
        to: &MultiLocation,
        context: &cumulus_primitives_core::XcmContext,
    ) -> Result<xcm_executor::Assets, XcmError> {
        log::trace!(
            target: LOG_TARGET,
            "transfer_asset asset: {asset:?}, from: {from:?}, to: {to:?}, context: {context:?}",
        );

        let Fungibility::NonFungible(asset_instance) = asset.fun else {
            return Err(XcmExecutorError::AssetNotHandled.into());
        };

        let from = <LocationToAccountIdOf<T, I>>::convert_location(from)
            .ok_or(XcmExecutorError::AccountIdConversionFailed)?;

        let to = <LocationToAccountIdOf<T, I>>::convert_location(to)
            .ok_or(XcmExecutorError::AccountIdConversionFailed)?;

        let token = Self::asset_instance_to_token(&asset.id, &asset_instance)?;

        Self::transfer_asset_instance(token, &from, &to).map(|()| asset.clone().into())
    }
}

type CategorizedTokenOf<T, I> =
    CategorizedToken<NativeTokenOf<T, I>, DerivativeTokenStatusOf<T, I>>;
type DerivativeIdStatusOf<T, I> = DerivativeIdStatus<TokenIdOf<T, I>>;
type DerivativeTokenStatusOf<T, I> = Token<CollectionIdOf<T, I>, DerivativeIdStatusOf<T, I>>;

// Common functions
impl<T: Config<I>, I: 'static> Pallet<T, I> {
    fn dispatch_error_to_xcm_error(error: DispatchError) -> XcmError {
        <T::NftInterface as NftInterface<T>>::PalletDispatchErrors::dispatch_error_to_xcm_error(
            error,
        )
    }

    /// Converts the XCM `asset_instance` to the corresponding NFT within the given collection.
    ///
    /// NOTE: for a local collection, the returned token ID may point to a non-existing NFT.
    fn asset_instance_to_token(
        asset_id: &AssetId,
        asset_instance: &AssetInstance,
    ) -> Result<CategorizedTokenOf<T, I>, XcmError> {
        let (collection_id, is_derivative) = Self::foreign_asset_to_collection(asset_id)
            .map(|collection_id| (collection_id, true))
            .or_else(|| {
                Self::local_asset_to_collection(asset_id)
                    .map(|collection_id| (collection_id, false))
            })
            .ok_or(XcmExecutorError::AssetIdConversionFailed)?;

        let token = if is_derivative {
            let derivative_token_status =
                Self::foreign_instance_to_derivative_status(&collection_id, asset_instance);

            CategorizedToken::Derivative {
                foreign_token: (Box::new(*asset_id), Box::new(*asset_instance)).into(),
                derivative_token: (collection_id, derivative_token_status).into(),
            }
        } else {
            CategorizedToken::Local(Token {
                collection_id,
                token_id: (*asset_instance)
                    .try_into()
                    .map_err(|_| XcmExecutorError::InstanceConversionFailed)?,
            })
        };

        Ok(token)
    }

    fn deposit_asset_instance(token: CategorizedTokenOf<T, I>, to: &T::AccountId) -> XcmResult {
        match token {
            CategorizedToken::Local(local_token) => Self::deposit_local_token(local_token, to),

            CategorizedToken::Derivative {
                foreign_token,
                derivative_token: derivative_token_status,
            } => Self::deposit_foreign_token(foreign_token, derivative_token_status, to),
        }
    }

    fn withdraw_asset_instance(token: CategorizedTokenOf<T, I>, from: &T::AccountId) -> XcmResult {
        match token {
            CategorizedToken::Local(local_token) => Self::withdraw_local_token(local_token, from),

            CategorizedToken::Derivative {
                foreign_token,
                derivative_token: derivative_token_status,
            } => {
                let derivative_token_id = derivative_token_status.token_id.existing()?;

                Self::withdraw_foreign_token(
                    foreign_token,
                    (derivative_token_status.collection_id, derivative_token_id).into(),
                    from,
                )
            }
        }
    }

    fn transfer_asset_instance(
        token: CategorizedTokenOf<T, I>,
        from: &T::AccountId,
        to: &T::AccountId,
    ) -> XcmResult {
        match token {
            CategorizedToken::Local(token) => {
                T::NftInterface::transfer(&token.collection_id, &token.token_id, from, to)
                    .map_err(Self::dispatch_error_to_xcm_error)?;

                Self::deposit_event(Event::Transferred {
                    token: CategorizedToken::Local(token),
                    from: from.clone(),
                    to: to.clone(),
                })
            }
            CategorizedToken::Derivative {
                foreign_token,
                derivative_token: derivative_token_status,
            } => {
                let collection_id = derivative_token_status.collection_id;
                let token_id = derivative_token_status.token_id.existing()?;

                T::NftInterface::transfer(&collection_id, &token_id, from, to)
                    .map_err(Self::dispatch_error_to_xcm_error)?;

                Self::deposit_event(Event::Transferred {
                    token: CategorizedToken::Derivative {
                        foreign_token,
                        derivative_token: (collection_id, token_id).into(),
                    },
                    from: from.clone(),
                    to: to.clone(),
                })
            }
        }

        Ok(())
    }
}

// local assets functions
impl<T: Config<I>, I: 'static> Pallet<T, I> {
    /// Converts the `asset_id` to the corresponding local NFT collection.
    ///
    /// The `asset_id` is considered to point to a local collection
    /// if it is in one of the following forms:
    /// * `<NftCollectionsLocation>/<Collection ID Junction>`
    /// * `../Parachain(<This Para ID>)/<NftCollectionsLocation>/<Collection ID Junction>`
    /// * `../../<UniversalLocation>/<NftCollectionsLocation>/<Collection ID Junction>`
    ///
    /// If the `asset_id` doesn't point to a local collection, `None` will be returned.
    ///
    /// NOTE: the `<Collection ID Junction>` in the valid forms specified above may point to a non-existing collection.
    /// Nonetheless, the conversion will be considered successful, and the collection ID will be returned.
    fn local_asset_to_collection(asset_id: &AssetId) -> Option<CollectionIdOf<T, I>> {
        let asset_id = Self::normalize_if_local_asset(*asset_id);

        let Concrete(asset_location) = asset_id else {
            return None;
        };

        if asset_location.parents > 0 {
            return None;
        }

        let prefix = MultiLocation::new(0, T::NftCollectionsLocation::get());

        (*asset_location.match_and_split(&prefix)?).try_into().ok()
    }

    fn deposit_local_token(local_token: NativeTokenOf<T, I>, to: &T::AccountId) -> XcmResult {
        T::NftInterface::transfer(
            &local_token.collection_id,
            &local_token.token_id,
            &Self::account_id(),
            to,
        )
        .map_err(Self::dispatch_error_to_xcm_error)?;

        Self::deposit_event(Event::Deposited {
            token: CategorizedToken::Local(local_token),
            beneficiary: to.clone(),
        });

        Ok(())
    }

    fn withdraw_local_token(local_token: NativeTokenOf<T, I>, from: &T::AccountId) -> XcmResult {
        T::NftInterface::transfer(
            &local_token.collection_id,
            &local_token.token_id,
            from,
            &Self::account_id(),
        )
        .map_err(Self::dispatch_error_to_xcm_error)?;

        Self::deposit_event(Event::Withdrawn {
            token: CategorizedToken::Local(local_token),
            benefactor: from.clone(),
        });

        Ok(())
    }
}

// foreign assets functions
impl<T: Config<I>, I: 'static> Pallet<T, I> {
    /// Deposits the foreign token.
    ///
    /// Either mints a new derivative or transfers the existing stashed derivative if one exists.
    ///
    /// If a new derivative is minted, it establishes the mapping
    /// between the foreign token and the derivative.
    fn deposit_foreign_token(
        foreign_token: ForeignToken,
        derivative_token_status: DerivativeTokenStatusOf<T, I>,
        to: &T::AccountId,
    ) -> XcmResult {
        let derivative_collection_id = derivative_token_status.collection_id;
        let derivative_id_status = derivative_token_status.token_id;

        let deposited_token_id = match derivative_id_status {
            DerivativeIdStatus::NotExists => {
                let token_id = T::NftInterface::mint_derivative(&derivative_collection_id, to)
                    .map_err(Self::dispatch_error_to_xcm_error)?;

                <DerivativeIdToForeignInstance<T, I>>::insert(
                    &derivative_collection_id,
                    &token_id,
                    *foreign_token.token_id,
                );

                <ForeignInstanceToDerivativeIdStatus<T, I>>::insert(
                    &derivative_collection_id,
                    *foreign_token.token_id,
                    DerivativeIdStatus::Active(token_id.clone()),
                );

                token_id
            }
            DerivativeIdStatus::Stashed(stashed_token_id) => {
                T::NftInterface::transfer(
                    &derivative_collection_id,
                    &stashed_token_id,
                    &Self::account_id(),
                    to,
                )
                .map_err(Self::dispatch_error_to_xcm_error)?;

                <ForeignInstanceToDerivativeIdStatus<T, I>>::insert(
                    &derivative_collection_id,
                    *foreign_token.token_id,
                    DerivativeIdStatus::Active(stashed_token_id.clone()),
                );

                stashed_token_id
            }
            DerivativeIdStatus::Active(_) => return Err(XcmError::NotDepositable),
        };

        Self::deposit_event(Event::Deposited {
            token: CategorizedToken::Derivative {
                foreign_token,
                derivative_token: (derivative_collection_id, deposited_token_id).into(),
            },
            beneficiary: to.clone(),
        });

        Ok(())
    }

    /// Withdraws the foreign token.
    ///
    /// If the [`NftInterface`] burns the derivative,
    /// this function will remove the mapping between the foreign token and the derivative.
    ///
    /// Otherwise, if the derivative should be stashed,
    /// this function transfers it to the xnft pallet account.
    fn withdraw_foreign_token(
        foreign_token: ForeignToken,
        derivative_token: NativeTokenOf<T, I>,
        from: &T::AccountId,
    ) -> XcmResult {
        let derivative_withdrawal = T::NftInterface::withdraw_derivative(
            &derivative_token.collection_id,
            &derivative_token.token_id,
            from,
        )
        .map_err(Self::dispatch_error_to_xcm_error)?;

        match derivative_withdrawal {
            DerivativeWithdrawal::Burned => {
                <DerivativeIdToForeignInstance<T, I>>::remove(
                    &derivative_token.collection_id,
                    &derivative_token.token_id,
                );
                <ForeignInstanceToDerivativeIdStatus<T, I>>::remove(
                    &derivative_token.collection_id,
                    *foreign_token.token_id,
                );
            }
            DerivativeWithdrawal::Stash => {
                T::NftInterface::transfer(
                    &derivative_token.collection_id,
                    &derivative_token.token_id,
                    from,
                    &Self::account_id(),
                )
                .map_err(Self::dispatch_error_to_xcm_error)?;

                <ForeignInstanceToDerivativeIdStatus<T, I>>::insert(
                    &derivative_token.collection_id,
                    *foreign_token.token_id,
                    DerivativeIdStatus::Stashed(derivative_token.token_id.clone()),
                );
            }
        }

        Self::deposit_event(Event::Withdrawn {
            token: CategorizedToken::Derivative {
                foreign_token,
                derivative_token,
            },
            benefactor: from.clone(),
        });

        Ok(())
    }
}
