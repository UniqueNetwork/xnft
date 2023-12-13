use frame_support::traits::Get;
use sp_runtime::{DispatchError, DispatchResult};
use xcm::v3::{prelude::*, Error as XcmError, Result as XcmResult};
use xcm_executor::traits::{ConvertLocation, Error as XcmExecutorError, TransactAsset};

use crate::{
    traits::{DerivativeWithdrawal, DispatchErrorToXcmError, NftInterface},
    CollectionIdOf, Config, DerivativeToForeignInstance, DerivativeTokenStatus,
    ForeignInstanceToDerivativeStatus, LocationToAccountId, Pallet, TokenIdOf,
};

const LOG_TARGET: &str = "xcm::xnft::transactor";

impl<T: Config> TransactAsset for Pallet<T> {
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

        let categorized_collection_id = Self::asset_to_collection(&asset.id)?;

        let to = <LocationToAccountId<T>>::convert_location(who)
            .ok_or(XcmExecutorError::AccountIdConversionFailed)?;

        Self::deposit_asset_instance(&categorized_collection_id, &asset_instance, &to)
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

        let categorized_collection_id = Self::asset_to_collection(&asset.id)?;

        let from = <LocationToAccountId<T>>::convert_location(who)
            .ok_or(XcmExecutorError::AccountIdConversionFailed)?;

        Self::withdraw_asset_instance(&categorized_collection_id, &asset_instance, &from)
            .map(|()| asset.clone().into())
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

        let categorized_collection_id = Self::asset_to_collection(&asset.id)?;

        let from = <LocationToAccountId<T>>::convert_location(from)
            .ok_or(XcmExecutorError::AccountIdConversionFailed)?;

        let to = <LocationToAccountId<T>>::convert_location(to)
            .ok_or(XcmExecutorError::AccountIdConversionFailed)?;

        let token_id =
            Self::asset_instance_to_token_id(&categorized_collection_id, &asset_instance)
                .ok_or(XcmExecutorError::InstanceConversionFailed)?;

        T::NftInterface::transfer(categorized_collection_id.plain_id(), &token_id, &from, &to)
            .map(|()| asset.clone().into())
            .map_err(Self::dispatch_error_to_xcm_error)
    }
}

pub enum CategorizedCollectionId<T: Config> {
    Local(CollectionIdOf<T>),
    Derivative(CollectionIdOf<T>),
}

impl<T: Config> CategorizedCollectionId<T> {
    fn plain_id(&self) -> &CollectionIdOf<T> {
        match self {
            Self::Local(id) => id,
            Self::Derivative(id) => id,
        }
    }
}

// Common functions
impl<T: Config> Pallet<T> {
    fn dispatch_error_to_xcm_error(error: DispatchError) -> XcmError {
        <T::NftInterface as NftInterface<T>>::PalletDispatchErrors::dispatch_error_to_xcm_error(
            error,
        )
    }

    /// Converts the XCM `asset_id` to the corresponding NFT collection (local or derivative).
    ///
    /// NOTE: A local collection ID may point to a non-existing collection.
    fn asset_to_collection(
        asset_id: &AssetId,
    ) -> Result<CategorizedCollectionId<T>, XcmExecutorError> {
        Self::foreign_asset_to_collection(asset_id)
            .map(CategorizedCollectionId::Derivative)
            .or_else(|| {
                Self::local_asset_to_collection(asset_id).map(CategorizedCollectionId::Local)
            })
            .ok_or(XcmExecutorError::AssetIdConversionFailed)
    }

    /// Converts the XCM `asset_instance` to the corresponding NFT within the given collection.
    ///
    /// NOTE: for a local collection, the returned token ID may point to a non-existing NFT.
    fn asset_instance_to_token_id(
        collection_id: &CategorizedCollectionId<T>,
        asset_instance: &AssetInstance,
    ) -> Option<TokenIdOf<T>> {
        match collection_id {
            CategorizedCollectionId::Local(_) => (*asset_instance).try_into().ok(),
            CategorizedCollectionId::Derivative(collection_id) => {
                Self::foreign_instance_to_derivative_status(collection_id, asset_instance)
                    .map(|status| status.token_id())
            }
        }
    }

    fn deposit_asset_instance(
        collection_id: &CategorizedCollectionId<T>,
        asset_instance: &AssetInstance,
        to: &T::AccountId,
    ) -> XcmResult {
        let token_id = Self::asset_instance_to_token_id(collection_id, asset_instance);

        match (collection_id, token_id) {
            (CategorizedCollectionId::Local(collection_id), Some(token_id)) => {
                Self::deposit_local_token(collection_id, &token_id, to)
                    .map_err(Self::dispatch_error_to_xcm_error)
            }
            (CategorizedCollectionId::Derivative(collection_id), None) => {
                Self::deposit_foreign_token(collection_id, asset_instance, to)
            }
            _ => Err(XcmExecutorError::InstanceConversionFailed.into()),
        }
    }

    fn withdraw_asset_instance(
        collection_id: &CategorizedCollectionId<T>,
        asset_instance: &AssetInstance,
        from: &T::AccountId,
    ) -> XcmResult {
        let token_id = Self::asset_instance_to_token_id(collection_id, asset_instance)
            .ok_or(XcmExecutorError::InstanceConversionFailed)?;

        match collection_id {
            CategorizedCollectionId::Local(collection_id) => {
                Self::withdraw_local_token(collection_id, &token_id, from)
            }
            CategorizedCollectionId::Derivative(collection_id) => {
                Self::withdraw_foreign_token(collection_id, &token_id, asset_instance, from)
            }
        }
        .map_err(Self::dispatch_error_to_xcm_error)
    }
}

// local assets functions
impl<T: Config> Pallet<T> {
    /// Converts the `asset_id` to the corresponding local NFT collection.
    ///
    /// The `asset_id` is considered to point to a local collection
    /// if it is in one of the following forms:
    /// * `<NftCollectionsLocation>/<Collection ID Junction>`
    /// * `../Parachain(<This Para ID>)/<NftCollectionsLocation>/<Collection ID Junction>`
    /// * `../../<UniversalLocation>/<NftCollectionsLocation>/<Collection ID Junction>`
    ///
    /// NOTE: the `<Collection ID Junction>` may point to a non-existing collection.
    /// Nonetheless, the conversion will be considered successful, and the collection ID will be returned.
    fn local_asset_to_collection(asset_id: &AssetId) -> Option<CollectionIdOf<T>> {
        let asset_id = Self::simplified_asset_id(*asset_id);

        let Concrete(asset_location) = asset_id else {
            return None;
        };

        if asset_location.parents > 0 {
            return None;
        }

        let prefix = MultiLocation::new(0, T::NftCollectionsLocation::get());

        (*asset_location.match_and_split(&prefix)?).try_into().ok()
    }

    fn deposit_local_token(
        collection_id: &CollectionIdOf<T>,
        token_id: &TokenIdOf<T>,
        to: &T::AccountId,
    ) -> DispatchResult {
        T::NftInterface::transfer(collection_id, token_id, &Self::account_id(), to)
    }

    fn withdraw_local_token(
        collection_id: &CollectionIdOf<T>,
        token_id: &TokenIdOf<T>,
        from: &T::AccountId,
    ) -> DispatchResult {
        T::NftInterface::transfer(collection_id, token_id, from, &Self::account_id())
    }
}

// foreign assets functions
impl<T: Config> Pallet<T> {
    /// Deposits the foreign token.
    ///
    /// Either mints a new derivative or transfers the existing stashed derivative if one exists.
    ///
    /// If a new derivative is minted, it establishes the mapping
    /// between the foreign token and the derivative.
    fn deposit_foreign_token(
        collection_id: &CollectionIdOf<T>,
        asset_instance: &AssetInstance,
        to: &T::AccountId,
    ) -> XcmResult {
        <ForeignInstanceToDerivativeStatus<T>>::try_mutate(
            collection_id,
            asset_instance,
            |status| {
                let token_id = match status {
                    None => {
                        let token_id = T::NftInterface::mint_derivative(collection_id, to)
                            .map_err(Self::dispatch_error_to_xcm_error)?;

                        <DerivativeToForeignInstance<T>>::insert(
                            collection_id,
                            &token_id,
                            asset_instance,
                        );

                        token_id
                    }
                    Some(DerivativeTokenStatus::Stashed(stashed_token_id)) => {
                        T::NftInterface::transfer(
                            collection_id,
                            stashed_token_id,
                            &Self::account_id(),
                            to,
                        )
                        .map_err(Self::dispatch_error_to_xcm_error)?;

                        stashed_token_id.clone()
                    }
                    Some(DerivativeTokenStatus::Active(_)) => return Err(XcmError::NotDepositable),
                };

                *status = Some(DerivativeTokenStatus::Active(token_id));

                Ok(())
            },
        )
    }

    /// Withdraws the foreign token.
    ///
    /// If the [`NftInterface`] burns the derivative,
    /// this function will remove the mapping between the foreign token and the derivative.
    ///
    /// Otherwise, if the derivative should be stashed,
    /// this function transfers it to the xnft pallet account.
    fn withdraw_foreign_token(
        collection_id: &CollectionIdOf<T>,
        derivative_token_id: &TokenIdOf<T>,
        foreign_asset_instance: &AssetInstance,
        from: &T::AccountId,
    ) -> DispatchResult {
        <ForeignInstanceToDerivativeStatus<T>>::try_mutate_exists(
            collection_id,
            foreign_asset_instance,
            |status| {
                match T::NftInterface::withdraw_derivative(
                    collection_id,
                    derivative_token_id,
                    from,
                )? {
                    DerivativeWithdrawal::Burned => {
                        *status = None;
                        <DerivativeToForeignInstance<T>>::remove(
                            collection_id,
                            derivative_token_id,
                        );
                    }
                    DerivativeWithdrawal::Stash => {
                        *status = Some(DerivativeTokenStatus::Stashed(derivative_token_id.clone()))
                    }
                }

                Ok(())
            },
        )
    }
}
