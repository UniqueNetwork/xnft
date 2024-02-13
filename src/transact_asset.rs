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
    traits::{DerivativeWithdrawal, DispatchErrorsConvert, NftEngine},
    CategorizedClassInstance, ClassIdOf, ClassInstance, ClassInstanceIdOf, ClassInstanceOf, Config,
    DerivativeStatus, DerivativeToForeignInstance, Event, ForeignAssetInstance,
    ForeignInstanceToDerivativeStatus, LocationToAccountIdOf, Pallet,
};

const LOG_TARGET: &str = "xcm::xnft::transactor";

impl<T: Config<I>, I: 'static> TransactAsset for Pallet<T, I> {
    fn deposit_asset(
        xcm_asset: &MultiAsset,
        who: &MultiLocation,
        context: Option<&XcmContext>,
    ) -> XcmResult {
        let xcm_asset = Self::simplify_asset(xcm_asset.clone());

        log::trace!(
            target: LOG_TARGET,
            "deposit_asset asset: {xcm_asset:?}, who: {who:?}, context: {context:?}",
        );

        let Fungibility::NonFungible(xcm_asset_instance) = xcm_asset.fun else {
            return Err(XcmExecutorError::AssetNotHandled.into());
        };

        let to = <LocationToAccountIdOf<T, I>>::convert_location(who)
            .ok_or(XcmExecutorError::AccountIdConversionFailed)?;

        let class_instance = Self::class_instance(&xcm_asset.id, &xcm_asset_instance)?;

        Self::deposit_class_instance(class_instance, &to)
    }

    fn withdraw_asset(
        xcm_asset: &MultiAsset,
        who: &MultiLocation,
        context: Option<&XcmContext>,
    ) -> Result<Assets, XcmError> {
        let xcm_asset = Self::simplify_asset(xcm_asset.clone());

        log::trace!(
            target: LOG_TARGET,
            "withdraw_asset asset: {xcm_asset:?}, who: {who:?}, context: {context:?}",
        );

        let Fungibility::NonFungible(xcm_asset_instance) = xcm_asset.fun else {
            return Err(XcmExecutorError::AssetNotHandled.into());
        };

        let from = <LocationToAccountIdOf<T, I>>::convert_location(who)
            .ok_or(XcmExecutorError::AccountIdConversionFailed)?;

        let class_instance = Self::class_instance(&xcm_asset.id, &xcm_asset_instance)?;

        Self::withdraw_class_instance(class_instance, &from).map(|()| xcm_asset.clone().into())
    }

    fn transfer_asset(
        xcm_asset: &MultiAsset,
        from: &MultiLocation,
        to: &MultiLocation,
        context: &XcmContext,
    ) -> Result<Assets, XcmError> {
        let xcm_asset = Self::simplify_asset(xcm_asset.clone());

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

        let class_instance = Self::class_instance(&xcm_asset.id, &xcm_asset_instance)?;

        Self::transfer_class_instance(class_instance, &from, &to).map(|()| xcm_asset.clone().into())
    }
}

type CategorizedClassInstanceOf<T, I> =
    CategorizedClassInstance<ClassInstanceOf<T, I>, DerivativeStatusOf<T, I>>;
type DerivativeIdStatusOf<T, I> = DerivativeStatus<ClassInstanceIdOf<T, I>>;
type DerivativeStatusOf<T, I> = ClassInstance<ClassIdOf<T, I>, DerivativeIdStatusOf<T, I>>;

// Common functions
impl<T: Config<I>, I: 'static> Pallet<T, I> {
    fn dispatch_error_to_xcm_error(error: DispatchError) -> XcmError {
        T::DispatchErrorsConvert::convert(error)
    }

    /// Converts the XCM `asset_instance` to the corresponding local class instance.
    ///
    /// NOTE: for a local class, the returned class instance ID may point to a non-existing NFT.
    fn class_instance(
        xcm_asset_id: &XcmAssetId,
        xcm_asset_instance: &XcmAssetInstance,
    ) -> Result<CategorizedClassInstanceOf<T, I>, XcmError> {
        let (class_id, is_derivative) = Self::foreign_asset_to_local_class(xcm_asset_id)
            .map(|class_id| (class_id, true))
            .or_else(|| Self::local_asset_to_class(xcm_asset_id).map(|class_id| (class_id, false)))
            .ok_or(XcmExecutorError::AssetIdConversionFailed)?;

        let class_instance = if is_derivative {
            let derivative_status =
                Self::foreign_instance_to_derivative_status(&class_id, xcm_asset_instance);

            CategorizedClassInstance::Derivative {
                foreign_asset_instance: Box::new((*xcm_asset_id, *xcm_asset_instance).into()),
                derivative: (class_id, derivative_status).into(),
            }
        } else {
            CategorizedClassInstance::Local(ClassInstance {
                class_id,
                instance_id: T::AssetInstanceConvert::convert(xcm_asset_instance)
                    .ok_or(XcmExecutorError::InstanceConversionFailed)?,
            })
        };

        Ok(class_instance)
    }

    fn deposit_class_instance(
        class_instance: CategorizedClassInstanceOf<T, I>,
        to: &T::AccountId,
    ) -> XcmResult {
        match class_instance {
            CategorizedClassInstance::Local(local_class_instance) => {
                Self::deposit_local_class_instance(local_class_instance, to)
            }

            CategorizedClassInstance::Derivative {
                foreign_asset_instance,
                derivative: derivative_status,
            } => {
                Self::deposit_foreign_asset_instance(foreign_asset_instance, derivative_status, to)
            }
        }
    }

    fn withdraw_class_instance(
        class_instance: CategorizedClassInstanceOf<T, I>,
        from: &T::AccountId,
    ) -> XcmResult {
        match class_instance {
            CategorizedClassInstance::Local(local_class_instance) => {
                Self::withdraw_local_class_instance(local_class_instance, from)
            }

            CategorizedClassInstance::Derivative {
                foreign_asset_instance,
                derivative: derivative_status,
            } => {
                let derivative_instance_id = derivative_status.instance_id.ensure_active()?;

                Self::withdraw_foreign_asset_instance(
                    foreign_asset_instance,
                    (derivative_status.class_id, derivative_instance_id).into(),
                    from,
                )
            }
        }
    }

    fn transfer_class_instance(
        class_instance: CategorizedClassInstanceOf<T, I>,
        from: &T::AccountId,
        to: &T::AccountId,
    ) -> XcmResult {
        match class_instance {
            CategorizedClassInstance::Local(class_instance) => {
                T::NftEngine::transfer_class_instance(
                    &class_instance.class_id,
                    &class_instance.instance_id,
                    from,
                    to,
                )
                .map_err(Self::dispatch_error_to_xcm_error)?;

                Self::deposit_event(Event::Transferred {
                    class_instance: CategorizedClassInstance::Local(class_instance),
                    from: from.clone(),
                    to: to.clone(),
                })
            }
            CategorizedClassInstance::Derivative {
                foreign_asset_instance,
                derivative: derivative_status,
            } => {
                let class_id = derivative_status.class_id;
                let instance_id = derivative_status.instance_id.ensure_active()?;

                T::NftEngine::transfer_class_instance(&class_id, &instance_id, from, to)
                    .map_err(Self::dispatch_error_to_xcm_error)?;

                Self::deposit_event(Event::Transferred {
                    class_instance: CategorizedClassInstance::Derivative {
                        foreign_asset_instance,
                        derivative: (class_id, instance_id).into(),
                    },
                    from: from.clone(),
                    to: to.clone(),
                })
            }
        }

        Ok(())
    }
}

// local classes functions
impl<T: Config<I>, I: 'static> Pallet<T, I> {
    /// Returns class ID for a local asset ID.
    /// The `xcm_asset_id` MUST be simplified before using this function.
    fn local_asset_to_class(xcm_asset_id: &XcmAssetId) -> Option<ClassIdOf<T, I>> {
        let Concrete(asset_location) = xcm_asset_id else {
            return None;
        };

        if asset_location.parents > 0 {
            return None;
        }

        let class_id = T::InteriorAssetIdConvert::convert(&asset_location.interior)?;

        Self::local_class_to_foreign_asset(&class_id)
            .is_none()
            .then_some(class_id)
    }

    fn deposit_local_class_instance(
        local_class_instance: ClassInstanceOf<T, I>,
        to: &T::AccountId,
    ) -> XcmResult {
        T::NftEngine::transfer_class_instance(
            &local_class_instance.class_id,
            &local_class_instance.instance_id,
            &Self::pallet_account_id(),
            to,
        )
        .map_err(Self::dispatch_error_to_xcm_error)?;

        Self::deposit_event(Event::Deposited {
            class_instance: CategorizedClassInstance::Local(local_class_instance),
            to: to.clone(),
        });

        Ok(())
    }

    fn withdraw_local_class_instance(
        local_class_instance: ClassInstanceOf<T, I>,
        from: &T::AccountId,
    ) -> XcmResult {
        T::NftEngine::transfer_class_instance(
            &local_class_instance.class_id,
            &local_class_instance.instance_id,
            from,
            &Self::pallet_account_id(),
        )
        .map_err(Self::dispatch_error_to_xcm_error)?;

        Self::deposit_event(Event::Withdrawn {
            class_instance: CategorizedClassInstance::Local(local_class_instance),
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
        foreign_asset_instance: Box<ForeignAssetInstance>,
        derivative_status: DerivativeStatusOf<T, I>,
        to: &T::AccountId,
    ) -> XcmResult {
        let derivative_class_id = derivative_status.class_id;
        let derivative_id_status = derivative_status.instance_id;

        let deposited_instance_id = match derivative_id_status {
            DerivativeStatus::NotExists => {
                let instance_id = T::NftEngine::mint_derivative(&derivative_class_id, to)
                    .map_err(Self::dispatch_error_to_xcm_error)?;

                <DerivativeToForeignInstance<T, I>>::insert(
                    &derivative_class_id,
                    &instance_id,
                    foreign_asset_instance.asset_instance,
                );

                <ForeignInstanceToDerivativeStatus<T, I>>::insert(
                    &derivative_class_id,
                    foreign_asset_instance.asset_instance,
                    DerivativeStatus::Active(instance_id.clone()),
                );

                instance_id
            }
            DerivativeStatus::Stashed(stashed_instance_id) => {
                T::NftEngine::transfer_class_instance(
                    &derivative_class_id,
                    &stashed_instance_id,
                    &Self::pallet_account_id(),
                    to,
                )
                .map_err(Self::dispatch_error_to_xcm_error)?;

                <ForeignInstanceToDerivativeStatus<T, I>>::insert(
                    &derivative_class_id,
                    foreign_asset_instance.asset_instance,
                    DerivativeStatus::Active(stashed_instance_id.clone()),
                );

                stashed_instance_id
            }
            DerivativeStatus::Active(_) => return Err(XcmError::NotDepositable),
        };

        Self::deposit_event(Event::Deposited {
            class_instance: CategorizedClassInstance::Derivative {
                foreign_asset_instance,
                derivative: (derivative_class_id, deposited_instance_id).into(),
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
        foreign_asset_instance: Box<ForeignAssetInstance>,
        derivative: ClassInstanceOf<T, I>,
        from: &T::AccountId,
    ) -> XcmResult {
        let derivative_withdrawal =
            T::NftEngine::withdraw_derivative(&derivative.class_id, &derivative.instance_id, from)
                .map_err(Self::dispatch_error_to_xcm_error)?;

        match derivative_withdrawal {
            DerivativeWithdrawal::Burned => {
                <DerivativeToForeignInstance<T, I>>::remove(
                    &derivative.class_id,
                    &derivative.instance_id,
                );
                <ForeignInstanceToDerivativeStatus<T, I>>::remove(
                    &derivative.class_id,
                    foreign_asset_instance.asset_instance,
                );
            }
            DerivativeWithdrawal::Stash => {
                T::NftEngine::transfer_class_instance(
                    &derivative.class_id,
                    &derivative.instance_id,
                    from,
                    &Self::pallet_account_id(),
                )
                .map_err(Self::dispatch_error_to_xcm_error)?;

                <ForeignInstanceToDerivativeStatus<T, I>>::insert(
                    &derivative.class_id,
                    foreign_asset_instance.asset_instance,
                    DerivativeStatus::Stashed(derivative.instance_id.clone()),
                );
            }
        }

        Self::deposit_event(Event::Withdrawn {
            class_instance: CategorizedClassInstance::Derivative {
                foreign_asset_instance,
                derivative,
            },
            from: from.clone(),
        });

        Ok(())
    }
}
