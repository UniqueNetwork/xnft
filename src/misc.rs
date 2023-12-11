//! This module contains miscellaneous tools
//! for integrating the xnft pallet into a Substrate chain.

use core::convert::Infallible;
use core::num::TryFromIntError;

use derive_more::{Deref, From};

use frame_support::{
    ensure,
    pallet_prelude::*,
    traits::{Contains, ProcessMessageError},
};
use sp_core::U256;
use sp_std::boxed::Box;
use xcm::v3::prelude::*;
use xcm_builder::{CreateMatcher, MatchXcm};
use xcm_executor::traits::{ConvertLocation, ConvertOrigin, ShouldExecute};

use crate::{
    Config, ForeignAssetToCollection, ForeignCollectionAllowedToRegister, Pallet, XnftOrigin,
};

/// Ensure that the foreign collection origin
/// registers the corresponding derivative collection.
pub struct EnsureForeignCollectionOrigin;
impl<OuterOrigin> EnsureOrigin<OuterOrigin> for EnsureForeignCollectionOrigin
where
    OuterOrigin: Into<Result<XnftOrigin, OuterOrigin>> + From<XnftOrigin>,
{
    type Success = ForeignCollectionAllowedToRegister;

    fn try_origin(o: OuterOrigin) -> Result<Self::Success, OuterOrigin> {
        o.into().map(|o| match o {
            XnftOrigin::ForeignCollection(id) => {
                ForeignCollectionAllowedToRegister::Definite(Box::new(id))
            }
        })
    }
}

/// Ensure that the `InnerOrigin` is allowed to register any derivative collection.
pub struct ForceRegisterOrigin<InnerOrigin>(PhantomData<InnerOrigin>);
impl<OuterOrigin, InnerOrigin: EnsureOrigin<OuterOrigin>> EnsureOrigin<OuterOrigin>
    for ForceRegisterOrigin<InnerOrigin>
{
    type Success = ForeignCollectionAllowedToRegister;

    fn try_origin(o: OuterOrigin) -> Result<Self::Success, OuterOrigin> {
        InnerOrigin::try_origin(o).map(|_| ForeignCollectionAllowedToRegister::Any)
    }
}

/// The converter from a foreign collection's reserve location to its account.
/// The collection's account is a sub-account of the xnft pallet's account.
pub struct ForeignCollectionToXnftSubAccountId<T: Config>(PhantomData<T>);
impl<T: Config> ConvertLocation<T::AccountId> for ForeignCollectionToXnftSubAccountId<T> {
    fn convert_location(location: &MultiLocation) -> Option<T::AccountId> {
        let asset_id: AssetId = (*location).into();

        <Pallet<T>>::foreign_asset_to_collection(asset_id)
            .and_then(<Pallet<T>>::collection_account_id)
    }
}

/// The converter from a foreign collection's reserve location
/// to the xnft foreign collection origin.
pub struct ForeignCollectionToXnftOrigin<T: Config>(PhantomData<T>);
impl<T: Config> ConvertOrigin<T::RuntimeOrigin> for ForeignCollectionToXnftOrigin<T>
where
    T::RuntimeOrigin: From<XnftOrigin>,
{
    fn convert_origin(
        origin: impl Into<MultiLocation>,
        kind: OriginKind,
    ) -> Result<T::RuntimeOrigin, MultiLocation> {
        if let OriginKind::Native = kind {
            let location = origin.into();
            let asset_id: AssetId = location.into();

            ensure!(
                <ForeignAssetToCollection<T>>::contains_key(asset_id),
                location,
            );

            Ok(XnftOrigin::ForeignCollection(asset_id).into())
        } else {
            Err(origin.into())
        }
    }
}

/// An XCM barrier that allows paid XCM Transact by a descended origin from specific locations.
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

/// An error during the conversion of a [`Junction`] to a collection ID.
pub enum JunctionConversionError<E> {
    /// Inner error of a specific conversion.
    InnerError(E),

    /// The supplied junction variant is not an expected one.
    InvalidJunctionVariant,
}

#[derive(Deref, From, Debug, PartialEq, Eq, Clone, Encode, Decode, TypeInfo, MaxEncodedLen)]
#[repr(transparent)]
/// A collection ID that can be created from the [`GeneralIndex`] junction.
pub struct GeneralIndexCollectionId<Id>(Id);
macro_rules! impl_try_from_general_index {
    ($ty:ty, $error:ty) => {
        impl TryFrom<Junction> for GeneralIndexCollectionId<$ty> {
            type Error = JunctionConversionError<$error>;

            fn try_from(junction: Junction) -> Result<Self, Self::Error> {
                match junction {
                    GeneralIndex(index) => Ok(Self(
                        index
                            .try_into()
                            .map_err(JunctionConversionError::InnerError)?,
                    )),
                    _ => Err(JunctionConversionError::InvalidJunctionVariant),
                }
            }
        }
    };
}
impl_try_from_general_index!(u32, TryFromIntError);
impl_try_from_general_index!(u64, TryFromIntError);
impl_try_from_general_index!(u128, Infallible);

#[derive(Deref, Debug, PartialEq, Eq, Clone, Encode, Decode, TypeInfo, MaxEncodedLen)]
#[repr(transparent)]
/// A collection ID that can be created from the [`AccountKey20`] junction.
pub struct AccountKey20CollectionId<Id, Network: Get<Option<NetworkId>>>(
    #[deref] Id,
    PhantomData<Network>,
);
impl<Id, Network: Get<Option<NetworkId>>> From<Id> for AccountKey20CollectionId<Id, Network> {
    fn from(id: Id) -> Self {
        Self(id, PhantomData)
    }
}
impl<Network> TryFrom<Junction> for AccountKey20CollectionId<[u8; 20], Network>
where
    Network: Get<Option<NetworkId>>,
{
    type Error = JunctionConversionError<Infallible>;

    fn try_from(junction: Junction) -> Result<Self, Self::Error> {
        match junction {
            AccountKey20 { network, key } if network == Network::get() => {
                Ok(Self(key, PhantomData))
            }
            _ => Err(JunctionConversionError::InvalidJunctionVariant),
        }
    }
}

#[derive(Deref, Debug, PartialEq, Eq, Clone, Encode, Decode, TypeInfo, MaxEncodedLen)]
#[repr(transparent)]
/// A collection ID that can be created from the [`AccountId32`] junction.
pub struct AccountId32CollectionId<Id, Network: Get<Option<NetworkId>>>(
    #[deref] Id,
    PhantomData<Network>,
);
impl<Id, Network: Get<Option<NetworkId>>> From<Id> for AccountId32CollectionId<Id, Network> {
    fn from(id: Id) -> Self {
        Self(id, PhantomData)
    }
}
impl<Network> TryFrom<Junction> for AccountId32CollectionId<[u8; 32], Network>
where
    Network: Get<Option<NetworkId>>,
{
    type Error = JunctionConversionError<Infallible>;

    fn try_from(junction: Junction) -> Result<Self, Self::Error> {
        match junction {
            AccountId32 { network, id } if network == Network::get() => Ok(Self(id, PhantomData)),
            _ => Err(JunctionConversionError::InvalidJunctionVariant),
        }
    }
}

#[derive(Deref, From, Debug, PartialEq, Eq, Clone, Encode, Decode, TypeInfo, MaxEncodedLen)]
#[repr(transparent)]
/// A collection ID that can be created from the [`GeneralKey`] junction with a 32-byte length.
pub struct GeneralKey32CollectionId<Id>(Id);
macro_rules! impl_try_from_general_key {
    ($ty:ty) => {
        impl TryFrom<Junction> for GeneralKey32CollectionId<$ty> {
            type Error = JunctionConversionError<Infallible>;

            fn try_from(junction: Junction) -> Result<Self, Self::Error> {
                match junction {
                    GeneralKey { length: 32, data } => Ok(Self(
                        data.try_into()
                            .map_err(JunctionConversionError::InnerError)?,
                    )),
                    _ => Err(JunctionConversionError::InvalidJunctionVariant),
                }
            }
        }
    };
}
impl_try_from_general_key!([u8; 32]);
impl_try_from_general_key!(U256);

/// An error during the conversion of an XCM [`AssetInstance`] to a a token ID.
pub enum InstanceConversionError<E> {
    /// Inner error of a specific conversion.
    InnerError(E),

    /// The supplied asset instance variant is not an expected one.
    InvalidInstanceVariant,
}

#[derive(Deref, From, Debug, PartialEq, Eq, Clone, Encode, Decode, TypeInfo, MaxEncodedLen)]
#[repr(transparent)]
/// A token ID that can be created from the [`Index`] asset instance.
pub struct IndexAssetInstance<Id>(Id);
macro_rules! impl_try_from_index {
    ($ty:ty, $error:ty) => {
        impl TryFrom<AssetInstance> for IndexAssetInstance<$ty> {
            type Error = InstanceConversionError<$error>;

            fn try_from(instance: AssetInstance) -> Result<Self, Self::Error> {
                match instance {
                    Index(index) => Ok(Self(
                        index
                            .try_into()
                            .map_err(InstanceConversionError::InnerError)?,
                    )),
                    _ => Err(InstanceConversionError::InvalidInstanceVariant),
                }
            }
        }
    };
}
impl_try_from_index!(u32, TryFromIntError);
impl_try_from_index!(u64, TryFromIntError);
impl_try_from_index!(u128, Infallible);

#[derive(Deref, From, Debug, PartialEq, Eq, Clone, Encode, Decode, TypeInfo, MaxEncodedLen)]
#[repr(transparent)]
/// A token ID that can be created from the [`Array4`] asset instance.
pub struct Array4AssetInstance([u8; 4]);
impl TryFrom<AssetInstance> for Array4AssetInstance {
    type Error = InstanceConversionError<Infallible>;

    fn try_from(instance: AssetInstance) -> Result<Self, Self::Error> {
        match instance {
            Array4(bytes) => Ok(Self(bytes)),
            _ => Err(InstanceConversionError::InvalidInstanceVariant),
        }
    }
}

#[derive(Deref, From, Debug, PartialEq, Eq, Clone, Encode, Decode, TypeInfo, MaxEncodedLen)]
#[repr(transparent)]
/// A token ID that can be created from the [`Array8`] asset instance.
pub struct Array8AssetInstance([u8; 8]);
impl TryFrom<AssetInstance> for Array8AssetInstance {
    type Error = InstanceConversionError<Infallible>;

    fn try_from(instance: AssetInstance) -> Result<Self, Self::Error> {
        match instance {
            Array8(bytes) => Ok(Self(bytes)),
            _ => Err(InstanceConversionError::InvalidInstanceVariant),
        }
    }
}

#[derive(Deref, From, Debug, PartialEq, Eq, Clone, Encode, Decode, TypeInfo, MaxEncodedLen)]
#[repr(transparent)]
/// A token ID that can be created from the [`Array16`] asset instance.
pub struct Array16AssetInstance([u8; 16]);
impl TryFrom<AssetInstance> for Array16AssetInstance {
    type Error = InstanceConversionError<Infallible>;

    fn try_from(instance: AssetInstance) -> Result<Self, Self::Error> {
        match instance {
            Array16(bytes) => Ok(Self(bytes)),
            _ => Err(InstanceConversionError::InvalidInstanceVariant),
        }
    }
}

#[derive(Deref, From, Debug, PartialEq, Eq, Clone, Encode, Decode, TypeInfo, MaxEncodedLen)]
#[repr(transparent)]
/// A token ID that can be created from the [`Array32`] asset instance.
pub struct Array32AssetInstance([u8; 32]);
impl TryFrom<AssetInstance> for Array32AssetInstance {
    type Error = InstanceConversionError<Infallible>;

    fn try_from(instance: AssetInstance) -> Result<Self, Self::Error> {
        match instance {
            Array32(bytes) => Ok(Self(bytes)),
            _ => Err(InstanceConversionError::InvalidInstanceVariant),
        }
    }
}
