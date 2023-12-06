use frame_support::{
    ensure,
    pallet_prelude::*,
    traits::{Contains, ProcessMessageError},
};
use xcm::v3::{AssetId, Instruction::*, MultiLocation, OriginKind, WeightLimit::*};
use xcm_builder::{CreateMatcher, MatchXcm};
use xcm_executor::traits::{ConvertLocation, ConvertOrigin, ShouldExecute};

use crate::{
    Config, ForeignAssetToCollection, ForeignCollectionAllowedToRegister, Pallet, RawOrigin,
};

pub struct EnsureCollectionOrigin;
impl<OuterOrigin> EnsureOrigin<OuterOrigin> for EnsureCollectionOrigin
where
    OuterOrigin: Into<Result<RawOrigin, OuterOrigin>> + From<RawOrigin>,
{
    type Success = ForeignCollectionAllowedToRegister;

    fn try_origin(o: OuterOrigin) -> Result<Self::Success, OuterOrigin> {
        o.into().and_then(|o| match o {
            RawOrigin::ForeignCollection(id) => {
                Ok(ForeignCollectionAllowedToRegister::Definite(id))
            }
        })
    }
}

pub struct ForceRegisterOrigin<CollectionId, O>(PhantomData<(CollectionId, O)>);
impl<OuterOrigin, CollectionId, O: EnsureOrigin<OuterOrigin>> EnsureOrigin<OuterOrigin>
    for ForceRegisterOrigin<CollectionId, O>
{
    type Success = ForeignCollectionAllowedToRegister;

    fn try_origin(o: OuterOrigin) -> Result<Self::Success, OuterOrigin> {
        O::try_origin(o).map(|_| ForeignCollectionAllowedToRegister::Any)
    }
}

pub struct ForeignCollectionToXnftSubAccountId<T: Config>(PhantomData<T>);
impl<T: Config> ConvertLocation<T::AccountId> for ForeignCollectionToXnftSubAccountId<T> {
    fn convert_location(location: &MultiLocation) -> Option<T::AccountId> {
        let asset_id: AssetId = location.clone().into();

        <Pallet<T>>::foreign_asset_to_collection(asset_id)
            .and_then(<Pallet<T>>::collection_account_id)
    }
}

pub struct ForeignCollectionToXnftOrigin<T: Config>(PhantomData<T>);
impl<T: Config> ConvertOrigin<T::RuntimeOrigin> for ForeignCollectionToXnftOrigin<T>
where
    T::RuntimeOrigin: From<RawOrigin>,
{
    fn convert_origin(
        origin: impl Into<MultiLocation>,
        kind: OriginKind,
    ) -> Result<T::RuntimeOrigin, MultiLocation> {
        if let OriginKind::Native = kind {
            let location = origin.into();
            let asset_id: AssetId = location.clone().into();

            ensure!(
                <ForeignAssetToCollection<T>>::contains_key(asset_id),
                location,
            );

            Ok(RawOrigin::ForeignCollection(asset_id).into())
        } else {
            Err(origin.into())
        }
    }
}

pub struct AllowDescendOriginPaidTransactFrom<T>(PhantomData<T>);
impl<T: Contains<MultiLocation>> ShouldExecute for AllowDescendOriginPaidTransactFrom<T> {
    fn should_execute<RuntimeCall>(
        origin: &MultiLocation,
        instructions: &mut [cumulus_primitives_core::Instruction<RuntimeCall>],
        max_weight: Weight,
        properties: &mut xcm_executor::traits::Properties,
    ) -> Result<(), frame_support::traits::ProcessMessageError> {
        log::trace!(
            target: "xcm::barriers",
            "AllowDescendOriginPaidTransactFrom origin: {:?}, instructions: {:?}, max_weight: {:?}, properties: {:?}",
            origin, instructions, max_weight, properties,
        );

        ensure!(T::contains(origin), ProcessMessageError::Unsupported);

        let end = instructions.len().min(4);
        instructions[..end]
            .matcher()
            .match_next_inst(|inst| match inst {
                DescendOrigin(_) => Ok(()),
                _ => Err(ProcessMessageError::BadFormat),
            })?
            .match_next_inst(|inst| match inst {
                WithdrawAsset(ref assets) if assets.len() == 1 => Ok(()),
                _ => Err(ProcessMessageError::BadFormat),
            })?
            .match_next_inst(|inst| match inst {
                BuyExecution {
                    weight_limit: Limited(ref mut weight),
                    ..
                } if weight.all_gte(max_weight) => {
                    *weight = max_weight;
                    Ok(())
                }
                BuyExecution {
                    ref mut weight_limit,
                    ..
                } if weight_limit == &Unlimited => {
                    *weight_limit = Limited(max_weight);
                    Ok(())
                }
                _ => Err(ProcessMessageError::Overweight(max_weight)),
            })?
            .match_next_inst(|inst| match inst {
                Transact {
                    origin_kind: OriginKind::Native,
                    ..
                } => Ok(()),
                _ => Err(ProcessMessageError::BadFormat),
            })?;

        Ok(())
    }
}
