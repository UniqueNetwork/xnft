use frame_support::{ensure, traits::Get};
use sp_runtime::{DispatchError, DispatchResult};
use xcm::v3::{prelude::*, Error as XcmError, Result as XcmResult};
use xcm_executor::traits::{ConvertLocation, Error as XcmExecutorError, TransactAsset};

use crate::{
    traits::{DerivativeWithdrawResult, DispatchErrorToXcmError, NftPallet},
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
        let asset = Self::simplified_multiasset(asset.clone());

        log::trace!(
            target: LOG_TARGET,
            "deposit_asset simplified(asset): {asset:?}, who: {who:?}, context: {context:?}",
        );

        let Fungibility::NonFungible(asset_instance) = asset.fun else {
            return Err(XcmExecutorError::AssetNotHandled.into());
        };

        let collection_locality = Self::asset_to_collection(&asset.id)?;

        let to = <LocationToAccountId<T>>::convert_location(who)
            .ok_or(XcmExecutorError::AccountIdConversionFailed)?;

        Self::deposit_asset_instance(&collection_locality, &asset_instance, &to)
    }

    fn withdraw_asset(
        original_asset: &MultiAsset,
        who: &MultiLocation,
        context: Option<&cumulus_primitives_core::XcmContext>,
    ) -> Result<xcm_executor::Assets, XcmError> {
        let asset = Self::simplified_multiasset(original_asset.clone());

        log::trace!(
            target: LOG_TARGET,
            "withdraw_asset simplified(asset): {asset:?}, who: {who:?}, context: {context:?}",
        );

        let Fungibility::NonFungible(asset_instance) = asset.fun else {
            return Err(XcmExecutorError::AssetNotHandled.into());
        };

        let collection_locality = Self::asset_to_collection(&asset.id)?;

        let from = <LocationToAccountId<T>>::convert_location(who)
            .ok_or(XcmExecutorError::AccountIdConversionFailed)?;

        Self::withdraw_asset_instance(&collection_locality, &asset_instance, &from)
            .map(|()| original_asset.clone().into())
    }

    fn transfer_asset(
        original_asset: &MultiAsset,
        from: &MultiLocation,
        to: &MultiLocation,
        context: &cumulus_primitives_core::XcmContext,
    ) -> Result<xcm_executor::Assets, XcmError> {
        let asset = Self::simplified_multiasset(original_asset.clone());

        log::trace!(
            target: LOG_TARGET,
            "transfer_asset simplified(asset): {asset:?}, from: {from:?}, to: {to:?}, context: {context:?}",
        );

        let Fungibility::NonFungible(asset_instance) = asset.fun else {
            return Err(XcmExecutorError::AssetNotHandled.into());
        };

        let collection_locality = Self::asset_to_collection(&asset.id)?;

        let from = <LocationToAccountId<T>>::convert_location(from)
            .ok_or(XcmExecutorError::AccountIdConversionFailed)?;

        let to = <LocationToAccountId<T>>::convert_location(to)
            .ok_or(XcmExecutorError::AccountIdConversionFailed)?;

        let token_id = Self::asset_instance_to_token_id(&collection_locality, &asset_instance)
            .ok_or(XcmExecutorError::InstanceConversionFailed)?;

        T::NftPallet::transfer(collection_locality.collection_id(), &token_id, &from, &to)
            .map(|()| original_asset.clone().into())
            .map_err(Self::dispatch_error_to_xcm_error)
    }
}

pub enum CollectionLocality<T: Config> {
    Local(CollectionIdOf<T>),
    Foreign(CollectionIdOf<T>),
}

impl<T: Config> CollectionLocality<T> {
    fn collection_id(&self) -> &CollectionIdOf<T> {
        match self {
            Self::Local(id) => id,
            Self::Foreign(id) => id,
        }
    }
}

// Common functions
impl<T: Config> Pallet<T> {
    fn dispatch_error_to_xcm_error(error: DispatchError) -> XcmError {
        <T::NftPallet as NftPallet<T>>::PalletDispatchErrors::to_xcm_error(error)
    }

    fn asset_to_collection(asset_id: &AssetId) -> Result<CollectionLocality<T>, XcmExecutorError> {
        Self::foreign_asset_to_collection(asset_id)
            .map(CollectionLocality::Foreign)
            .or_else(|| Self::local_asset_to_collection(asset_id).map(CollectionLocality::Local))
            .ok_or(XcmExecutorError::AssetIdConversionFailed)
    }

    fn asset_instance_to_token_id(
        locality: &CollectionLocality<T>,
        asset_instance: &AssetInstance,
    ) -> Option<TokenIdOf<T>> {
        match locality {
            CollectionLocality::Local(_) => (*asset_instance).try_into().ok(),
            CollectionLocality::Foreign(collection_id) => {
                Self::foreign_instance_to_derivative_status(collection_id, asset_instance)
                    .map(|status| status.token_id())
            }
        }
    }

    fn deposit_asset_instance(
        locality: &CollectionLocality<T>,
        asset_instance: &AssetInstance,
        to: &T::AccountId,
    ) -> XcmResult {
        let token_id = Self::asset_instance_to_token_id(locality, asset_instance);

        match (locality, token_id) {
            (CollectionLocality::Local(collection_id), Some(token_id)) => {
                Self::deposit_local_token(collection_id, &token_id, to)
                    .map_err(Self::dispatch_error_to_xcm_error)
            }
            (CollectionLocality::Foreign(collection_id), None) => {
                Self::deposit_foreign_token(collection_id, asset_instance, to)
            }
            _ => Err(XcmExecutorError::InstanceConversionFailed.into()),
        }
    }

    fn withdraw_asset_instance(
        locality: &CollectionLocality<T>,
        asset_instance: &AssetInstance,
        from: &T::AccountId,
    ) -> XcmResult {
        let token_id = Self::asset_instance_to_token_id(locality, asset_instance)
            .ok_or(XcmExecutorError::InstanceConversionFailed)?;

        match locality {
            CollectionLocality::Local(collection_id) => {
                Self::withdraw_local_token(collection_id, &token_id, from)
            }
            CollectionLocality::Foreign(collection_id) => {
                Self::withdraw_foreign_token(collection_id, &token_id, asset_instance, from)
            }
        }
        .map_err(Self::dispatch_error_to_xcm_error)
    }
}

// local assets functions
impl<T: Config> Pallet<T> {
    /// TODO doc
    ///
    /// NOTE: `asset_id` MUST be simplified relative to the `UniversalLocation`.
    fn local_asset_to_collection(simplified_asset_id: &AssetId) -> Option<CollectionIdOf<T>> {
        let Concrete(simplified_asset_location) = simplified_asset_id else {
            return None;
        };

        let prefix = MultiLocation::new(0, T::NftCollectionsLocation::get());

        (*simplified_asset_location.match_and_split(&prefix)?)
            .try_into()
            .ok()
    }

    fn deposit_local_token(
        collection_id: &CollectionIdOf<T>,
        token_id: &TokenIdOf<T>,
        to: &T::AccountId,
    ) -> DispatchResult {
        T::NftPallet::transfer(collection_id, token_id, &Self::account_id(), to)
    }

    fn withdraw_local_token(
        collection_id: &CollectionIdOf<T>,
        token_id: &TokenIdOf<T>,
        from: &T::AccountId,
    ) -> DispatchResult {
        T::NftPallet::transfer(collection_id, token_id, from, &Self::account_id())
    }
}

// foreign assets functions
impl<T: Config> Pallet<T> {
    fn deposit_foreign_token(
        collection_id: &CollectionIdOf<T>,
        asset_instance: &AssetInstance,
        to: &T::AccountId,
    ) -> XcmResult {
        <ForeignInstanceToDerivativeStatus<T>>::try_mutate(
            collection_id,
            asset_instance,
            |status| {
                let stashed_token_id = match status.as_ref() {
                    Some(DerivativeTokenStatus::Stashed(stashed_token_id)) => {
                        Some(stashed_token_id)
                    }
                    Some(DerivativeTokenStatus::Active(_)) => return Err(XcmError::NotDepositable),
                    None => None,
                };

                let token_id =
                    T::NftPallet::deposit_derivative(collection_id, stashed_token_id, to)
                        .map_err(Self::dispatch_error_to_xcm_error)?;

                match stashed_token_id {
                    Some(stashed_token_id) => {
                        ensure!(token_id == *stashed_token_id, XcmError::NotDepositable)
                    }
                    None => <DerivativeToForeignInstance<T>>::insert(
                        collection_id,
                        &token_id,
                        asset_instance,
                    ),
                }

                *status = Some(DerivativeTokenStatus::Active(token_id));

                Ok(())
            },
        )
    }

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
                match T::NftPallet::withdraw_derivative(collection_id, derivative_token_id, from)? {
                    DerivativeWithdrawResult::Burned => {
                        *status = None;
                        <DerivativeToForeignInstance<T>>::remove(
                            collection_id,
                            derivative_token_id,
                        );
                    }
                    DerivativeWithdrawResult::Stashed => {
                        *status = Some(DerivativeTokenStatus::Stashed(derivative_token_id.clone()))
                    }
                }

                Ok(())
            },
        )
    }
}
