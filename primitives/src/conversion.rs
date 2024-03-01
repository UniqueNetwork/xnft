//! This module contains conversion utilities.

use frame_support::pallet_prelude::*;
use sp_runtime::traits::MaybeEquivalence;
use xcm::v3::prelude::*;

fn ensure_correct_prefix<Prefix: Get<InteriorMultiLocation>>(
    location: &InteriorMultiLocation,
) -> Option<InteriorMultiLocation> {
    let prefix = Prefix::get();

    prefix
        .iter()
        .enumerate()
        .all(|(index, junction)| location.at(index) == Some(junction))
        .then_some(prefix)
}

/// The converter to match the [`InteriorMultiLocation`] as prefixed `GeneralIndex`
/// and to convert the index into a value of the `AssetId` type
/// using the `ConvertAssetId` converter.
pub struct InteriorGeneralIndex<Prefix, AssetId, ConvertAssetId>(
    PhantomData<(Prefix, AssetId, ConvertAssetId)>,
);
impl<
        Prefix: Get<InteriorMultiLocation>,
        AssetId,
        ConvertAssetId: MaybeEquivalence<u128, AssetId>,
    > MaybeEquivalence<InteriorMultiLocation, AssetId>
    for InteriorGeneralIndex<Prefix, AssetId, ConvertAssetId>
{
    fn convert(id: &InteriorMultiLocation) -> Option<AssetId> {
        let prefix = ensure_correct_prefix::<Prefix>(id)?;
        match id.at(prefix.len()) {
            Some(Junction::GeneralIndex(index)) => ConvertAssetId::convert(index),
            _ => None,
        }
    }
    fn convert_back(what: &AssetId) -> Option<InteriorMultiLocation> {
        let mut location = Prefix::get();
        let index = ConvertAssetId::convert_back(what)?;
        location.push(Junction::GeneralIndex(index)).ok()?;
        Some(location)
    }
}

/// The converter to match the [`InteriorMultiLocation`] as prefixed `AccountKey20`
/// and to convert the account key into a value of the `AssetId` type
/// using the `ConvertAssetId` converter.
pub struct InteriorAccountKey20<Prefix, AssetId, ConvertAssetId>(
    PhantomData<(Prefix, AssetId, ConvertAssetId)>,
);
impl<
        Prefix: Get<InteriorMultiLocation>,
        AssetId,
        ConvertAssetId: MaybeEquivalence<(Option<NetworkId>, [u8; 20]), AssetId>,
    > MaybeEquivalence<InteriorMultiLocation, AssetId>
    for InteriorAccountKey20<Prefix, AssetId, ConvertAssetId>
{
    fn convert(id: &InteriorMultiLocation) -> Option<AssetId> {
        let prefix = ensure_correct_prefix::<Prefix>(id)?;
        match id.at(prefix.len()) {
            Some(Junction::AccountKey20 { network, key }) => {
                ConvertAssetId::convert(&(*network, *key))
            }
            _ => None,
        }
    }
    fn convert_back(what: &AssetId) -> Option<InteriorMultiLocation> {
        let mut location = Prefix::get();
        let (network, key) = ConvertAssetId::convert_back(what)?;
        location
            .push(Junction::AccountKey20 { network, key })
            .ok()?;
        Some(location)
    }
}

/// The converter to match the [`InteriorMultiLocation`] as prefixed `AccountId32`
/// and to convert the account ID into a value of the `AssetId` type
/// using the `ConvertAssetId` converter.
pub struct InteriorAccountId32<Prefix, AssetId, ConvertAssetId>(
    PhantomData<(Prefix, AssetId, ConvertAssetId)>,
);
impl<
        Prefix: Get<InteriorMultiLocation>,
        AssetId,
        ConvertAssetId: MaybeEquivalence<(Option<NetworkId>, [u8; 32]), AssetId>,
    > MaybeEquivalence<InteriorMultiLocation, AssetId>
    for InteriorAccountId32<Prefix, AssetId, ConvertAssetId>
{
    fn convert(id: &InteriorMultiLocation) -> Option<AssetId> {
        let prefix = ensure_correct_prefix::<Prefix>(id)?;
        match id.at(prefix.len()) {
            Some(Junction::AccountId32 { network, id }) => {
                ConvertAssetId::convert(&(*network, *id))
            }
            _ => None,
        }
    }
    fn convert_back(what: &AssetId) -> Option<InteriorMultiLocation> {
        let mut location = Prefix::get();
        let (network, id) = ConvertAssetId::convert_back(what)?;
        location.push(Junction::AccountId32 { network, id }).ok()?;
        Some(location)
    }
}

/// The converter to match the [`InteriorMultiLocation`] as prefixed `GeneralKey`
/// and to convert the general key into a value of the `AssetId` type
/// using the `ConvertAssetId` converter.
pub struct InteriorGeneralKey<Prefix, AssetId, ConvertAssetId>(
    PhantomData<(Prefix, AssetId, ConvertAssetId)>,
);

impl<
        Prefix: Get<InteriorMultiLocation>,
        AssetId,
        ConvertAssetId: MaybeEquivalence<(u8, [u8; 32]), AssetId>,
    > MaybeEquivalence<InteriorMultiLocation, AssetId>
    for InteriorGeneralKey<Prefix, AssetId, ConvertAssetId>
{
    fn convert(id: &InteriorMultiLocation) -> Option<AssetId> {
        let prefix = ensure_correct_prefix::<Prefix>(id)?;
        match id.at(prefix.len()) {
            Some(Junction::GeneralKey { length, data }) => {
                ConvertAssetId::convert(&(*length, *data))
            }
            _ => None,
        }
    }
    fn convert_back(what: &AssetId) -> Option<InteriorMultiLocation> {
        let mut location = Prefix::get();
        let (length, data) = ConvertAssetId::convert_back(what)?;
        location.push(Junction::GeneralKey { length, data }).ok()?;
        Some(location)
    }
}

/// The converter to match the [`AssetInstance`] as `Index`
/// and to convert the index into a value of the `InstanceId` type
/// using the `ConvertAssetInstance` converter.
pub struct IndexAssetInstance<InstanceId, ConvertAssetInstance>(
    PhantomData<(InstanceId, ConvertAssetInstance)>,
);
impl<InstanceId, ConvertAssetInstance: MaybeEquivalence<u128, InstanceId>>
    MaybeEquivalence<AssetInstance, InstanceId>
    for IndexAssetInstance<InstanceId, ConvertAssetInstance>
{
    fn convert(instance: &AssetInstance) -> Option<InstanceId> {
        match instance {
            AssetInstance::Index(instance) => ConvertAssetInstance::convert(instance),
            _ => None,
        }
    }

    fn convert_back(instance: &InstanceId) -> Option<AssetInstance> {
        ConvertAssetInstance::convert_back(instance).map(AssetInstance::Index)
    }
}

/// The converter to match the [`AssetInstance`] as `Array4`
/// and to convert the array into a value of the `InstanceId` type
/// using the `ConvertAssetInstance` converter.
pub struct Array4AssetInstance<InstanceId, ConvertAssetInstance>(
    PhantomData<(InstanceId, ConvertAssetInstance)>,
);
impl<InstanceId, ConvertAssetInstance: MaybeEquivalence<[u8; 4], InstanceId>>
    MaybeEquivalence<AssetInstance, InstanceId>
    for Array4AssetInstance<InstanceId, ConvertAssetInstance>
{
    fn convert(instance: &AssetInstance) -> Option<InstanceId> {
        match instance {
            AssetInstance::Array4(instance) => ConvertAssetInstance::convert(instance),
            _ => None,
        }
    }

    fn convert_back(instance: &InstanceId) -> Option<AssetInstance> {
        ConvertAssetInstance::convert_back(instance).map(AssetInstance::Array4)
    }
}

/// The converter to match the [`AssetInstance`] as `Array8`
/// and to convert the array into a value of the `InstanceId` type
/// using the `ConvertAssetInstance` converter.
pub struct Array8AssetInstance<InstanceId, ConvertAssetInstance>(
    PhantomData<(InstanceId, ConvertAssetInstance)>,
);
impl<InstanceId, ConvertAssetInstance: MaybeEquivalence<[u8; 8], InstanceId>>
    MaybeEquivalence<AssetInstance, InstanceId>
    for Array8AssetInstance<InstanceId, ConvertAssetInstance>
{
    fn convert(instance: &AssetInstance) -> Option<InstanceId> {
        match instance {
            AssetInstance::Array8(instance) => ConvertAssetInstance::convert(instance),
            _ => None,
        }
    }

    fn convert_back(instance: &InstanceId) -> Option<AssetInstance> {
        ConvertAssetInstance::convert_back(instance).map(AssetInstance::Array8)
    }
}

/// The converter to match the [`AssetInstance`] as `Array16`
/// and to convert the array into a value of the `InstanceId` type
/// using the `ConvertAssetInstance` converter.
pub struct Array16AssetInstance<InstanceId, ConvertAssetInstance>(
    PhantomData<(InstanceId, ConvertAssetInstance)>,
);
impl<InstanceId, ConvertAssetInstance: MaybeEquivalence<[u8; 16], InstanceId>>
    MaybeEquivalence<AssetInstance, InstanceId>
    for Array16AssetInstance<InstanceId, ConvertAssetInstance>
{
    fn convert(instance: &AssetInstance) -> Option<InstanceId> {
        match instance {
            AssetInstance::Array16(instance) => ConvertAssetInstance::convert(instance),
            _ => None,
        }
    }

    fn convert_back(instance: &InstanceId) -> Option<AssetInstance> {
        ConvertAssetInstance::convert_back(instance).map(AssetInstance::Array16)
    }
}

/// The converter to match the [`AssetInstance`] as `Array32`
/// and to convert the array into a value of the `InstanceId` type
/// using the `ConvertAssetInstance` converter.
pub struct Array32AssetInstance<InstanceId, ConvertAssetInstance>(
    PhantomData<(InstanceId, ConvertAssetInstance)>,
);
impl<InstanceId, ConvertAssetInstance: MaybeEquivalence<[u8; 32], InstanceId>>
    MaybeEquivalence<AssetInstance, InstanceId>
    for Array32AssetInstance<InstanceId, ConvertAssetInstance>
{
    fn convert(instance: &AssetInstance) -> Option<InstanceId> {
        match instance {
            AssetInstance::Array32(instance) => ConvertAssetInstance::convert(instance),
            _ => None,
        }
    }

    fn convert_back(instance: &InstanceId) -> Option<AssetInstance> {
        ConvertAssetInstance::convert_back(instance).map(AssetInstance::Array32)
    }
}
