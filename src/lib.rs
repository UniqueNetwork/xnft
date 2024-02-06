#![warn(missing_docs)]
#![cfg_attr(not(feature = "std"), no_std)]

//! The xnft pallet is a generalized NFT XCM Asset Transactor.
//! It can be integrated into any Substrate chain implementing the [`NftEngine`] trait.

use frame_support::{ensure, pallet_prelude::*, traits::EnsureOriginWithArg, PalletId};
use frame_system::pallet_prelude::*;
use sp_runtime::{traits::AccountIdConversion, DispatchResult};
use sp_std::boxed::Box;
use xcm::{
    v3::prelude::{AssetId as XcmAssetId, AssetInstance as XcmAssetInstance, *},
    VersionedAssetId,
};
use xcm_executor::traits::{ConvertLocation, Error as XcmExecutorError};

use traits::{AssetCreationWeight, NftEngine};

pub use pallet::*;

pub mod conversion;
pub mod traits;

#[allow(missing_docs)]
pub mod weights;

mod transact_asset;

#[cfg(feature = "runtime-benchmarks")]
#[allow(missing_docs)]
pub mod benchmarking;

type NftEngineOf<T, I> = <T as Config<I>>::NftEngine;
type LocalAssetIdOf<T, I> = <NftEngineOf<T, I> as NftEngine<T>>::AssetId;
type LocalInstanceIdOf<T, I> = <NftEngineOf<T, I> as NftEngine<T>>::AssetInstanceId;

type LocationToAccountIdOf<T, I> = <T as Config<I>>::LocationToAccountId;
type AssetCreationWeightOf<T, I> =
    <<T as Config<I>>::NftEngine as NftEngine<T>>::AssetCreationWeight;

#[frame_support::pallet]
pub mod pallet {
    use sp_runtime::traits::MaybeEquivalence;
    use weights::WeightInfo;

    use super::*;

    #[pallet::config]
    pub trait Config<I: 'static = ()>: frame_system::Config {
        /// An implementation of the chain's NFT Engine.
        type NftEngine: NftEngine<Self>;

        type InteriorAssetIdConvert: MaybeEquivalence<
            InteriorMultiLocation,
            <Self::NftEngine as NftEngine<Self>>::AssetId,
        >;

        type InteriorAssetInstanceConvert: MaybeEquivalence<
            XcmAssetInstance,
            <Self::NftEngine as NftEngine<Self>>::AssetInstanceId,
        >;

        /// The xnft pallet instance's ID.
        type PalletId: Get<PalletId>;

        /// The chain's Universal Location.
        type UniversalLocation: Get<InteriorMultiLocation>;

        /// A converter from a multilocation to the chain's account ID.
        type LocationToAccountId: ConvertLocation<Self::AccountId>;

        /// An origin allowed to register foreign NFT assets.
        type ForeignAssetRegisterOrigin: EnsureOriginWithArg<Self::RuntimeOrigin, AssetId>;

        /// The aggregated event type of the runtime.
        type RuntimeEvent: From<Event<Self, I>>
            + IsType<<Self as frame_system::Config>::RuntimeEvent>;

        /// The weight info.
        type WeightInfo: WeightInfo;
    }

    /// XNFT errors.
    #[pallet::error]
    pub enum Error<T, I = ()> {
        /// The asset is already registered.
        AssetAlreadyRegistered,

        /// Is it impossible to register a local asset as a foreign one.
        AttemptToRegisterLocalAsset,

        /// The given asset ID could not be converted into the current XCM version.
        BadAssetId,
    }

    #[pallet::event]
    #[pallet::generate_deposit(pub(crate) fn deposit_event)]
    pub enum Event<T: Config<I>, I: 'static = ()> {
        /// The given foreign asset is registered.
        ForeignAssetRegistered {
            /// The XCM asset ID of the registered foreign asset.
            foreign_asset_id: Box<AssetId>,

            /// The derivative asset ID of the registered foreign asset.
            derivative_asset_id: LocalAssetIdOf<T, I>,
        },

        /// An asset instance is deposited.
        Deposited {
            /// The asset instance in question.
            asset_instance:
                CategorizedAssetInstance<LocalAssetInstanceOf<T, I>, LocalAssetInstanceOf<T, I>>,

            /// The account to whom the NFT derivative is deposited.
            to: T::AccountId,
        },

        /// An asset instance is withdrawn.
        Withdrawn {
            /// The asset instance in question.
            asset_instance:
                CategorizedAssetInstance<LocalAssetInstanceOf<T, I>, LocalAssetInstanceOf<T, I>>,

            /// The account from whom the NFT derivative is withdrawn.
            from: T::AccountId,
        },

        /// An asset instance is transferred.
        Transferred {
            /// The asset instance in question.
            asset_instance:
                CategorizedAssetInstance<LocalAssetInstanceOf<T, I>, LocalAssetInstanceOf<T, I>>,

            /// The account from whom the NFT derivative is withdrawn.
            from: T::AccountId,

            /// The account to whom the NFT derivative is deposited.
            to: T::AccountId,
        },
    }

    #[pallet::storage]
    #[pallet::getter(fn foreign_to_local_asset)]
    pub type ForeignToLocalAsset<T: Config<I>, I: 'static = ()> =
        StorageMap<_, Blake2_128Concat, xcm::v3::AssetId, LocalAssetIdOf<T, I>, OptionQuery>;

    #[pallet::storage]
    #[pallet::getter(fn local_to_foreign_asset)]
    pub type LocalToForeignAsset<T: Config<I>, I: 'static = ()> =
        StorageMap<_, Blake2_128Concat, LocalAssetIdOf<T, I>, xcm::v3::AssetId, OptionQuery>;

    #[pallet::storage]
    #[pallet::getter(fn foreign_instance_to_derivative_status)]
    pub type ForeignInstanceToDerivativeIdStatus<T: Config<I>, I: 'static = ()> = StorageDoubleMap<
        _,
        Blake2_128Concat,
        LocalAssetIdOf<T, I>,
        Blake2_128Concat,
        xcm::v3::AssetInstance,
        DerivativeIdStatus<LocalInstanceIdOf<T, I>>,
        ValueQuery,
    >;

    #[pallet::storage]
    #[pallet::getter(fn derivative_to_foreign_instance)]
    pub type DerivativeIdToForeignInstance<T: Config<I>, I: 'static = ()> = StorageDoubleMap<
        _,
        Blake2_128Concat,
        LocalAssetIdOf<T, I>,
        Blake2_128Concat,
        LocalInstanceIdOf<T, I>,
        xcm::v3::AssetInstance,
        OptionQuery,
    >;

    #[pallet::pallet]
    pub struct Pallet<T, I = ()>(_);

    #[pallet::call]
    impl<T: Config<I>, I: 'static> Pallet<T, I> {
        /// Registers a foreign non-fungible asset.
        ///
        /// Creates a derivative asset on this chain
        /// backed by the foreign asset identified by the `versioned_foreign_asset`.
        #[pallet::call_index(0)]
        #[pallet::weight(T::WeightInfo::foreign_asset_registration_checks()
            .saturating_add(AssetCreationWeightOf::<T, I>::asset_creation_weight(derivative_asset_data))
			.saturating_add(T::DbWeight::get().writes(3)))]
        pub fn register_foreign_asset(
            origin: OriginFor<T>,
            versioned_foreign_asset: Box<VersionedAssetId>,
            derivative_asset_data: <T::NftEngine as NftEngine<T>>::AssetData,
        ) -> DispatchResult {
            let foreign_asset_id =
                Self::foreign_asset_registration_checks(origin, versioned_foreign_asset)?;

            let derivative_asset_id =
                T::NftEngine::register_asset(&Self::account_id(), derivative_asset_data)?;

            <ForeignToLocalAsset<T, I>>::insert(foreign_asset_id, &derivative_asset_id);
            <LocalToForeignAsset<T, I>>::insert(&derivative_asset_id, foreign_asset_id);

            Self::deposit_event(Event::ForeignAssetRegistered {
                foreign_asset_id: Box::new(foreign_asset_id),
                derivative_asset_id,
            });

            Ok(())
        }
    }
}

impl<T: Config<I>, I: 'static> Pallet<T, I> {
    /// The xnft pallet's account ID derived from the pallet ID.
    pub fn account_id() -> T::AccountId {
        <T as Config<I>>::PalletId::get().into_account_truncating()
    }

    /// This function normalizes the `asset_id` if it represent an asset local to this chain.
    /// The normal form for local assets is: `parents: 0, interior: <junctions>`.
    ///
    /// An asset is considered local if its reserve location points to the interior of this chain.
    /// For instance:
    /// * `parents: 0, interior: Xn(...)` --> Already in the normal form.
    /// * `parents: 1, interior: Xn+1(Parachain(<this chain ID>), ...)` --> Will be converted.
    /// * `parents: 2, interior: Xn+2(GlobalConsensus(<Network ID>), Parachain(<this chain ID>), ...)` --> Will be converted.
    ///
    /// This function uses the `UniversalLocation` to check if the `asset_id` is a local asset.
    ///
    /// If the `asset_id` is NOT a local asset, it will be returned unmodified.
    pub fn normalize_if_local_asset(mut asset_id: AssetId) -> AssetId {
        if let AssetId::Concrete(location) = &mut asset_id {
            let context = T::UniversalLocation::get();
            location.simplify(&context);
        }

        asset_id
    }

    /// Check if the foreign asset can be registered.
    pub fn foreign_asset_registration_checks(
        origin: OriginFor<T>,
        versioned_foreign_asset: Box<VersionedAssetId>,
    ) -> Result<AssetId, DispatchError> {
        let foreign_asset: AssetId = versioned_foreign_asset
            .as_ref()
            .clone()
            .try_into()
            .map_err(|()| Error::<T, I>::BadAssetId)?;

        let normalized_asset = Self::normalize_if_local_asset(foreign_asset);

        if let AssetId::Concrete(location) = normalized_asset {
            ensure!(
                location.parents > 0,
                <Error<T, I>>::AttemptToRegisterLocalAsset
            );
        }

        T::ForeignAssetRegisterOrigin::ensure_origin(origin, &normalized_asset)?;

        ensure!(
            !<ForeignToLocalAsset<T, I>>::contains_key(normalized_asset),
            <Error<T, I>>::AssetAlreadyRegistered,
        );

        Ok(normalized_asset)
    }
}

#[derive(Default, Debug, PartialEq, Eq, Clone, Encode, Decode, MaxEncodedLen, TypeInfo)]
/// The status of a derivative asset instance ID.
pub enum DerivativeIdStatus<InstanceId> {
    /// The given derivative ID is active,
    /// meaning it is backed by the original asset and owned by a user on this chain.
    Active(InstanceId),

    /// The given derivative ID is stashed,
    /// meaning the original asset does not back it now,
    /// and no one on this chain can own this derivative.
    ///
    /// This asset instance ID will become active when the original asset
    /// is deposited into this chain again.
    Stashed(InstanceId),

    /// No derivative ID exists.
    #[default]
    NotExists,
}

impl<InstanceId> DerivativeIdStatus<InstanceId> {
    fn ensure_active(self) -> Result<InstanceId, XcmError> {
        match self {
            Self::Active(id) => Ok(id),
            Self::Stashed(_) => Err(XcmError::NoPermission),
            Self::NotExists => Err(XcmExecutorError::InstanceConversionFailed.into()),
        }
    }
}

/// An NFT complete identification.
#[derive(Debug, PartialEq, Eq, Clone, Encode, Decode, MaxEncodedLen, TypeInfo)]
pub struct AssetInstance<LocalAssetId, InstanceId> {
    /// The asset ID of the instance.
    pub asset_id: LocalAssetId,

    /// The ID the asset instance.
    pub instance_id: InstanceId,
}

impl<LocalAssetId, InstanceId> From<(LocalAssetId, InstanceId)>
    for AssetInstance<LocalAssetId, InstanceId>
{
    fn from((asset_id, instance_id): (LocalAssetId, InstanceId)) -> Self {
        Self {
            asset_id,
            instance_id,
        }
    }
}

/// A categorized asset instance represents either
/// a local asset instance or a derivative asset instance corresponding to a foreign one on a remote chain.
#[derive(Debug, PartialEq, Eq, Clone, Encode, Decode, MaxEncodedLen, TypeInfo)]
pub enum CategorizedAssetInstance<LocalInstance, DerivativeInstance> {
    /// A local asset instance.
    Local(LocalInstance),

    /// A derivative asset instance corresponding to a foreign NFT on a remote chain.
    Derivative {
        /// The foreign asset instance to which the derivative corresponds.
        foreign_asset_instance: ForeignAssetInstance,

        /// The derivative asset instance on this chain corresponding to the foreign one.
        derivative: DerivativeInstance,
    },
}

type ForeignAssetInstance = AssetInstance<Box<XcmAssetId>, Box<XcmAssetInstance>>;
type LocalAssetInstanceOf<T, I> = AssetInstance<LocalAssetIdOf<T, I>, LocalInstanceIdOf<T, I>>;
