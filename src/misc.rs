//! This module contains miscellaneous tools
//! for integrating the xnft pallet into a Substrate chain.

use core::convert::Infallible;
use core::num::TryFromIntError;

use derive_more::{Deref, From};

use frame_support::pallet_prelude::*;
use sp_core::U256;
use xcm::v3::prelude::*;

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
