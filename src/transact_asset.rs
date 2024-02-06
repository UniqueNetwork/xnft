use cumulus_primitives_core::XcmContext;
use sp_runtime::{traits::MaybeEquivalence, DispatchError};
use sp_std::boxed::Box;
use xcm::v3::{
    prelude::{AssetId as XcmAssetId, AssetInstance as XcmAssetInstance, *},
    Error as XcmError, Result as XcmResult,
};
use xcm_executor::{
    traits::{ConvertLocation, Error as XcmExecutorError, TransactAsset},
    Assets,
};

use crate::{
    traits::{DerivativeWithdrawal, DispatchErrorToXcmError, NftEngine},
    AssetInstance, CategorizedAssetInstance, Config, DerivativeIdStatus,
    DerivativeIdToForeignInstance, Event, ForeignAssetInstance,
    ForeignInstanceToDerivativeIdStatus, LocalAssetIdOf, LocalAssetInstanceOf, LocalInstanceIdOf,
    LocationToAccountIdOf, Pallet,
};

const LOG_TARGET: &str = "xcm::xnft::transactor";

impl<T: Config<I>, I: 'static> TransactAsset for Pallet<T, I> {
    fn deposit_asset(
        xcm_asset: &MultiAsset,
        who: &MultiLocation,
        context: Option<&XcmContext>,
    ) -> XcmResult {
        log::trace!(
            target: LOG_TARGET,
            "deposit_asset asset: {xcm_asset:?}, who: {who:?}, context: {context:?}",
        );

        let Fungibility::NonFungible(xcm_asset_instance) = xcm_asset.fun else {
            return Err(XcmExecutorError::AssetNotHandled.into());
        };

        let to = <LocationToAccountIdOf<T, I>>::convert_location(who)
            .ok_or(XcmExecutorError::AccountIdConversionFailed)?;

        let asset_instance = Self::asset_instance(&xcm_asset.id, &xcm_asset_instance)?;

        Self::deposit_asset_instance(asset_instance, &to)
    }

    fn withdraw_asset(
        xcm_asset: &MultiAsset,
        who: &MultiLocation,
        context: Option<&XcmContext>,
    ) -> Result<Assets, XcmError> {
        log::trace!(
            target: LOG_TARGET,
            "withdraw_asset asset: {xcm_asset:?}, who: {who:?}, context: {context:?}",
        );

        let Fungibility::NonFungible(xcm_asset_instance) = xcm_asset.fun else {
            return Err(XcmExecutorError::AssetNotHandled.into());
        };

        let from = <LocationToAccountIdOf<T, I>>::convert_location(who)
            .ok_or(XcmExecutorError::AccountIdConversionFailed)?;

        let asset_instance = Self::asset_instance(&xcm_asset.id, &xcm_asset_instance)?;

        Self::withdraw_asset_instance(asset_instance, &from).map(|()| xcm_asset.clone().into())
    }

    fn transfer_asset(
        xcm_asset: &MultiAsset,
        from: &MultiLocation,
        to: &MultiLocation,
        context: &XcmContext,
    ) -> Result<Assets, XcmError> {
        log::trace!(
            target: LOG_TARGET,
            "transfer_asset asset: {xcm_asset:?}, from: {from:?}, to: {to:?}, context: {context:?}",
        );

        let Fungibility::NonFungible(xcm_asset_instance) = xcm_asset.fun else {
            return Err(XcmExecutorError::AssetNotHandled.into());
        };

        let from = <LocationToAccountIdOf<T, I>>::convert_location(from)
            .ok_or(XcmExecutorError::AccountIdConversionFailed)?;

        let to = <LocationToAccountIdOf<T, I>>::convert_location(to)
            .ok_or(XcmExecutorError::AccountIdConversionFailed)?;

        let asset_instance = Self::asset_instance(&xcm_asset.id, &xcm_asset_instance)?;

        Self::transfer_asset_instance(asset_instance, &from, &to).map(|()| xcm_asset.clone().into())
    }
}

type CategorizedAssetInstanceOf<T, I> =
    CategorizedAssetInstance<LocalAssetInstanceOf<T, I>, DerivativeStatusOf<T, I>>;
type DerivativeIdStatusOf<T, I> = DerivativeIdStatus<LocalInstanceIdOf<T, I>>;
type DerivativeStatusOf<T, I> = AssetInstance<LocalAssetIdOf<T, I>, DerivativeIdStatusOf<T, I>>;

// Common functions
impl<T: Config<I>, I: 'static> Pallet<T, I> {
    fn dispatch_error_to_xcm_error(error: DispatchError) -> XcmError {
        <T::NftEngine as NftEngine<T>>::PalletDispatchErrors::dispatch_error_to_xcm_error(error)
    }

    /// Converts the XCM `asset_instance` to the corresponding local asset instance.
    ///
    /// NOTE: for a local asset, the returned asset instance ID may point to a non-existing NFT.
    fn asset_instance(
        xcm_asset_id: &XcmAssetId,
        xcm_asset_instance: &XcmAssetInstance,
    ) -> Result<CategorizedAssetInstanceOf<T, I>, XcmError> {
        let (asset_id, is_derivative) = Self::foreign_to_local_asset(xcm_asset_id)
            .map(|asset_id| (asset_id, true))
            .or_else(|| Self::local_asset(xcm_asset_id).map(|asset_id| (asset_id, false)))
            .ok_or(XcmExecutorError::AssetIdConversionFailed)?;

        let asset_instance = if is_derivative {
            let derivative_status =
                Self::foreign_instance_to_derivative_status(&asset_id, xcm_asset_instance);

            CategorizedAssetInstance::Derivative {
                foreign_asset_instance: (Box::new(*xcm_asset_id), Box::new(*xcm_asset_instance))
                    .into(),
                derivative: (asset_id, derivative_status).into(),
            }
        } else {
            CategorizedAssetInstance::Local(AssetInstance {
                asset_id,
                instance_id: T::InteriorAssetInstanceConvert::convert(xcm_asset_instance)
                    .ok_or(XcmExecutorError::InstanceConversionFailed)?,
            })
        };

        Ok(asset_instance)
    }

    fn deposit_asset_instance(
        asset_instance: CategorizedAssetInstanceOf<T, I>,
        to: &T::AccountId,
    ) -> XcmResult {
        match asset_instance {
            CategorizedAssetInstance::Local(local_asset_instance) => {
                Self::deposit_local_asset_instance(local_asset_instance, to)
            }

            CategorizedAssetInstance::Derivative {
                foreign_asset_instance,
                derivative: derivative_status,
            } => {
                Self::deposit_foreign_asset_instance(foreign_asset_instance, derivative_status, to)
            }
        }
    }

    fn withdraw_asset_instance(
        asset_instance: CategorizedAssetInstanceOf<T, I>,
        from: &T::AccountId,
    ) -> XcmResult {
        match asset_instance {
            CategorizedAssetInstance::Local(local_asset_instance) => {
                Self::withdraw_local_asset_instance(local_asset_instance, from)
            }

            CategorizedAssetInstance::Derivative {
                foreign_asset_instance,
                derivative: derivative_status,
            } => {
                let derivative_instance_id = derivative_status.instance_id.ensure_active()?;

                Self::withdraw_foreign_asset_instance(
                    foreign_asset_instance,
                    (derivative_status.asset_id, derivative_instance_id).into(),
                    from,
                )
            }
        }
    }

    fn transfer_asset_instance(
        asset_instance: CategorizedAssetInstanceOf<T, I>,
        from: &T::AccountId,
        to: &T::AccountId,
    ) -> XcmResult {
        match asset_instance {
            CategorizedAssetInstance::Local(asset_instance) => {
                T::NftEngine::transfer_asset_instance(
                    &asset_instance.asset_id,
                    &asset_instance.instance_id,
                    from,
                    to,
                )
                .map_err(Self::dispatch_error_to_xcm_error)?;

                Self::deposit_event(Event::Transferred {
                    asset_instance: CategorizedAssetInstance::Local(asset_instance),
                    from: from.clone(),
                    to: to.clone(),
                })
            }
            CategorizedAssetInstance::Derivative {
                foreign_asset_instance,
                derivative: derivative_status,
            } => {
                let asset_id = derivative_status.asset_id;
                let instance_id = derivative_status.instance_id.ensure_active()?;

                T::NftEngine::transfer_asset_instance(&asset_id, &instance_id, from, to)
                    .map_err(Self::dispatch_error_to_xcm_error)?;

                Self::deposit_event(Event::Transferred {
                    asset_instance: CategorizedAssetInstance::Derivative {
                        foreign_asset_instance,
                        derivative: (asset_id, instance_id).into(),
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
    fn local_asset(xcm_asset_id: &XcmAssetId) -> Option<LocalAssetIdOf<T, I>> {
        let xcm_asset_id = Self::normalize_if_local_asset(*xcm_asset_id);

        let Concrete(asset_location) = xcm_asset_id else {
            return None;
        };

        if asset_location.parents > 0 {
            return None;
        }

        T::InteriorAssetIdConvert::convert(&asset_location.interior)
    }

    fn deposit_local_asset_instance(
        local_asset_instance: LocalAssetInstanceOf<T, I>,
        to: &T::AccountId,
    ) -> XcmResult {
        T::NftEngine::transfer_asset_instance(
            &local_asset_instance.asset_id,
            &local_asset_instance.instance_id,
            &Self::account_id(),
            to,
        )
        .map_err(Self::dispatch_error_to_xcm_error)?;

        Self::deposit_event(Event::Deposited {
            asset_instance: CategorizedAssetInstance::Local(local_asset_instance),
            to: to.clone(),
        });

        Ok(())
    }

    fn withdraw_local_asset_instance(
        local_asset_instance: LocalAssetInstanceOf<T, I>,
        from: &T::AccountId,
    ) -> XcmResult {
        T::NftEngine::transfer_asset_instance(
            &local_asset_instance.asset_id,
            &local_asset_instance.instance_id,
            from,
            &Self::account_id(),
        )
        .map_err(Self::dispatch_error_to_xcm_error)?;

        Self::deposit_event(Event::Withdrawn {
            asset_instance: CategorizedAssetInstance::Local(local_asset_instance),
            from: from.clone(),
        });

        Ok(())
    }
}

// foreign assets functions
impl<T: Config<I>, I: 'static> Pallet<T, I> {
    /// Deposits the foreign asset instance.
    ///
    /// Either mints a new derivative or transfers the existing stashed derivative if one exists.
    ///
    /// If a new derivative is minted, it establishes the mapping
    /// between the foreign asset instance and the derivative.
    fn deposit_foreign_asset_instance(
        foreign_asset_instance: ForeignAssetInstance,
        derivative_status: DerivativeStatusOf<T, I>,
        to: &T::AccountId,
    ) -> XcmResult {
        let derivative_asset_id = derivative_status.asset_id;
        let derivative_id_status = derivative_status.instance_id;

        let deposited_instance_id = match derivative_id_status {
            DerivativeIdStatus::NotExists => {
                let instance_id = T::NftEngine::mint_derivative(&derivative_asset_id, to)
                    .map_err(Self::dispatch_error_to_xcm_error)?;

                <DerivativeIdToForeignInstance<T, I>>::insert(
                    &derivative_asset_id,
                    &instance_id,
                    *foreign_asset_instance.instance_id,
                );

                <ForeignInstanceToDerivativeIdStatus<T, I>>::insert(
                    &derivative_asset_id,
                    *foreign_asset_instance.instance_id,
                    DerivativeIdStatus::Active(instance_id.clone()),
                );

                instance_id
            }
            DerivativeIdStatus::Stashed(stashed_instance_id) => {
                T::NftEngine::transfer_asset_instance(
                    &derivative_asset_id,
                    &stashed_instance_id,
                    &Self::account_id(),
                    to,
                )
                .map_err(Self::dispatch_error_to_xcm_error)?;

                <ForeignInstanceToDerivativeIdStatus<T, I>>::insert(
                    &derivative_asset_id,
                    *foreign_asset_instance.instance_id,
                    DerivativeIdStatus::Active(stashed_instance_id.clone()),
                );

                stashed_instance_id
            }
            DerivativeIdStatus::Active(_) => return Err(XcmError::NotDepositable),
        };

        Self::deposit_event(Event::Deposited {
            asset_instance: CategorizedAssetInstance::Derivative {
                foreign_asset_instance,
                derivative: (derivative_asset_id, deposited_instance_id).into(),
            },
            to: to.clone(),
        });

        Ok(())
    }

    /// Withdraws the foreign asset instance.
    ///
    /// If the [`NftEngine`] burns the derivative,
    /// this function will remove the mapping between
    /// the foreign asset instance and the derivative.
    ///
    /// Otherwise, if the derivative should be stashed,
    /// this function transfers it to the xnft pallet account.
    fn withdraw_foreign_asset_instance(
        foreign_asset_instance: ForeignAssetInstance,
        derivative: LocalAssetInstanceOf<T, I>,
        from: &T::AccountId,
    ) -> XcmResult {
        let derivative_withdrawal =
            T::NftEngine::withdraw_derivative(&derivative.asset_id, &derivative.instance_id, from)
                .map_err(Self::dispatch_error_to_xcm_error)?;

        match derivative_withdrawal {
            DerivativeWithdrawal::Burned => {
                <DerivativeIdToForeignInstance<T, I>>::remove(
                    &derivative.asset_id,
                    &derivative.instance_id,
                );
                <ForeignInstanceToDerivativeIdStatus<T, I>>::remove(
                    &derivative.asset_id,
                    *foreign_asset_instance.instance_id,
                );
            }
            DerivativeWithdrawal::Stash => {
                T::NftEngine::transfer_asset_instance(
                    &derivative.asset_id,
                    &derivative.instance_id,
                    from,
                    &Self::account_id(),
                )
                .map_err(Self::dispatch_error_to_xcm_error)?;

                <ForeignInstanceToDerivativeIdStatus<T, I>>::insert(
                    &derivative.asset_id,
                    *foreign_asset_instance.instance_id,
                    DerivativeIdStatus::Stashed(derivative.instance_id.clone()),
                );
            }
        }

        Self::deposit_event(Event::Withdrawn {
            asset_instance: CategorizedAssetInstance::Derivative {
                foreign_asset_instance,
                derivative,
            },
            from: from.clone(),
        });

        Ok(())
    }
}
