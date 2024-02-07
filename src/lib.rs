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

use traits::{DerivativeClassCreation, NftEngine};

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
type NftEngineAccountId<T, I> = <NftEngineOf<T, I> as NftEngine>::AccountId;
type ClassIdOf<T, I> = <NftEngineOf<T, I> as NftEngine>::ClassId;
type ClassInstanceIdOf<T, I> = <NftEngineOf<T, I> as NftEngine>::ClassInstanceId;

type LocationToAccountIdOf<T, I> = <T as Config<I>>::LocationToAccountId;
type DerivativeClassCreationOf<T, I> = <NftEngineOf<T, I> as NftEngine>::DerivativeClassCreation;
type DerivativeClassDataOf<T, I> = <DerivativeClassCreationOf<T, I> as DerivativeClassCreation<
    ClassIdOf<T, I>,
>>::DerivativeClassData;

#[frame_support::pallet]
pub mod pallet {
    use sp_runtime::traits::MaybeEquivalence;
    use weights::WeightInfo;

    use crate::traits::DispatchErrorToXcmError;

    use super::*;

    #[pallet::config]
    pub trait Config<I: 'static = ()>: frame_system::Config {
        /// The aggregated event type of the runtime.
        type RuntimeEvent: From<Event<Self, I>>
            + IsType<<Self as frame_system::Config>::RuntimeEvent>;

        /// The xnft pallet instance's ID.
        type PalletId: Get<PalletId>;

        /// An implementation of the chain's NFT Engine.
        type NftEngine: NftEngine;

        type InteriorAssetIdConvert: MaybeEquivalence<
            InteriorMultiLocation,
            <Self::NftEngine as NftEngine>::ClassId,
        >;

        type AssetInstanceConvert: MaybeEquivalence<
            XcmAssetInstance,
            <Self::NftEngine as NftEngine>::ClassInstanceId,
        >;

        /// The chain's Universal Location.
        type UniversalLocation: Get<InteriorMultiLocation>;

        /// A converter from a multilocation to the chain's account ID.
        type LocationToAccountId: ConvertLocation<NftEngineAccountId<Self, I>>;

        /// Pallet dispatch errors that are convertible to XCM errors.
        ///
        /// A type implementing [`IntoXcmError`], [`PalletError`], and [`Decode`] traits
        /// or a tuple constructed from such types can be used.
        ///
        /// This type allows the xnft pallet to decode certain pallet errors into proper XCM errors.
        ///
        /// The [`FailedToTransactAsset`](XcmError::FailedToTransactAsset) is a fallback
        /// when the dispatch error can't be decoded into any of the specified dispatch error types.
        type PalletDispatchErrors: DispatchErrorToXcmError<Self>;

        /// An origin allowed to register foreign NFT assets.
        type ForeignAssetRegisterOrigin: EnsureOriginWithArg<Self::RuntimeOrigin, XcmAssetId>;

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
            foreign_asset_id: Box<XcmAssetId>,

            /// The derivative class ID of the registered foreign asset.
            derivative_class_id: ClassIdOf<T, I>,
        },

        /// A class instance is deposited.
        Deposited {
            /// The class instance in question.
            class_instance: CategorizedClassInstance<ClassInstanceOf<T, I>, ClassInstanceOf<T, I>>,

            /// The account to whom the NFT derivative is deposited.
            to: NftEngineAccountId<T, I>,
        },

        /// A class instance is withdrawn.
        Withdrawn {
            /// The class instance in question.
            class_instance: CategorizedClassInstance<ClassInstanceOf<T, I>, ClassInstanceOf<T, I>>,

            /// The account from whom the NFT derivative is withdrawn.
            from: NftEngineAccountId<T, I>,
        },

        /// A class instance is transferred.
        Transferred {
            /// The class instance in question.
            class_instance: CategorizedClassInstance<ClassInstanceOf<T, I>, ClassInstanceOf<T, I>>,

            /// The account from whom the NFT derivative is withdrawn.
            from: NftEngineAccountId<T, I>,

            /// The account to whom the NFT derivative is deposited.
            to: NftEngineAccountId<T, I>,
        },
    }

    #[pallet::storage]
    #[pallet::getter(fn foreign_asset_to_local_class)]
    pub type ForeignAssetToLocalClass<T: Config<I>, I: 'static = ()> =
        StorageMap<_, Blake2_128Concat, xcm::v3::AssetId, ClassIdOf<T, I>, OptionQuery>;

    #[pallet::storage]
    #[pallet::getter(fn local_class_to_foreign_asset)]
    pub type LocalClassToForeignAsset<T: Config<I>, I: 'static = ()> =
        StorageMap<_, Blake2_128Concat, ClassIdOf<T, I>, xcm::v3::AssetId, OptionQuery>;

    #[pallet::storage]
    #[pallet::getter(fn foreign_instance_to_derivative_status)]
    pub type ForeignInstanceToDerivativeIdStatus<T: Config<I>, I: 'static = ()> = StorageDoubleMap<
        _,
        Blake2_128Concat,
        ClassIdOf<T, I>,
        Blake2_128Concat,
        xcm::v3::AssetInstance,
        DerivativeIdStatus<ClassInstanceIdOf<T, I>>,
        ValueQuery,
    >;

    #[pallet::storage]
    #[pallet::getter(fn derivative_to_foreign_instance)]
    pub type DerivativeIdToForeignInstance<T: Config<I>, I: 'static = ()> = StorageDoubleMap<
        _,
        Blake2_128Concat,
        ClassIdOf<T, I>,
        Blake2_128Concat,
        ClassInstanceIdOf<T, I>,
        xcm::v3::AssetInstance,
        OptionQuery,
    >;

    #[pallet::pallet]
    pub struct Pallet<T, I = ()>(_);

    #[pallet::call]
    impl<T: Config<I>, I: 'static> Pallet<T, I> {
        /// Registers a foreign non-fungible asset.
        ///
        /// Creates a derivative class on this chain
        /// backed by the foreign asset identified by the `versioned_foreign_asset`.
        #[pallet::call_index(0)]
        #[pallet::weight(T::WeightInfo::foreign_asset_registration_checks()
            .saturating_add(DerivativeClassCreationOf::<T, I>::class_creation_weight(derivative_class_data))
			.saturating_add(T::DbWeight::get().writes(3)))]
        pub fn register_foreign_asset(
            origin: OriginFor<T>,
            versioned_foreign_asset: Box<VersionedAssetId>,
            derivative_class_data: DerivativeClassDataOf<T, I>,
        ) -> DispatchResult {
            let foreign_asset_id =
                Self::foreign_asset_registration_checks(origin, versioned_foreign_asset)?;

            let derivative_class_id =
                DerivativeClassCreationOf::<T, I>::create_derivative_class(derivative_class_data)?;

            <ForeignAssetToLocalClass<T, I>>::insert(foreign_asset_id, &derivative_class_id);
            <LocalClassToForeignAsset<T, I>>::insert(&derivative_class_id, foreign_asset_id);

            Self::deposit_event(Event::ForeignAssetRegistered {
                foreign_asset_id: Box::new(foreign_asset_id),
                derivative_class_id,
            });

            Ok(())
        }
    }
}

impl<T: Config<I>, I: 'static> Pallet<T, I> {
    /// This function simplifies the `asset_id` reserve location
    /// relative to the `UniversalLocation` of this chain.
    ///
    /// See `fn simplify` in [MultiLocation].
    fn simplify_asset_id(mut asset_id: XcmAssetId) -> XcmAssetId {
        if let XcmAssetId::Concrete(location) = &mut asset_id {
            let context = T::UniversalLocation::get();
            location.simplify(&context);
        }

        asset_id
    }

    /// This function simplifies the `asset` reserve location
    /// relative to the `UniversalLocation` of this chain.
    ///
    /// See `fn simplify` in [MultiLocation].
    fn simplify_asset(mut xcm_asset: MultiAsset) -> MultiAsset {
        MultiAsset {
            id: Self::simplify_asset_id(xcm_asset.id),
            ..xcm_asset
        }
    }

    /// Check if the foreign asset can be registered.
    fn foreign_asset_registration_checks(
        origin: OriginFor<T>,
        versioned_foreign_asset: Box<VersionedAssetId>,
    ) -> Result<XcmAssetId, DispatchError> {
        let foreign_asset_id: XcmAssetId = versioned_foreign_asset
            .as_ref()
            .clone()
            .try_into()
            .map_err(|()| Error::<T, I>::BadAssetId)?;

        let simplified_asset_id = Self::simplify_asset_id(foreign_asset_id);

        if let XcmAssetId::Concrete(location) = simplified_asset_id {
            ensure!(
                location.parents > 0,
                <Error<T, I>>::AttemptToRegisterLocalAsset
            );
        }

        T::ForeignAssetRegisterOrigin::ensure_origin(origin, &simplified_asset_id)?;

        ensure!(
            !<ForeignAssetToLocalClass<T, I>>::contains_key(simplified_asset_id),
            <Error<T, I>>::AssetAlreadyRegistered,
        );

        Ok(simplified_asset_id)
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
    /// This class instance ID will become active when the original asset
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
pub struct ClassInstance<ClassId, InstanceId> {
    /// The class ID of the instance.
    pub class_id: ClassId,

    /// The ID the class instance.
    pub instance_id: InstanceId,
}

impl<ClassId, InstanceId> From<(ClassId, InstanceId)> for ClassInstance<ClassId, InstanceId> {
    fn from((class_id, instance_id): (ClassId, InstanceId)) -> Self {
        Self {
            class_id,
            instance_id,
        }
    }
}

type ClassInstanceOf<T, I> = ClassInstance<ClassIdOf<T, I>, ClassInstanceIdOf<T, I>>;

/// A foreign NFT complete identification.
#[derive(Debug, PartialEq, Eq, Clone, Encode, Decode, MaxEncodedLen, TypeInfo)]
pub struct ForeignAssetInstance {
    /// The asset ID of the foreign instance.
    pub asset_id: XcmAssetId,

    /// The foreign asset instance.
    pub asset_instance: XcmAssetInstance,
}

impl From<(XcmAssetId, XcmAssetInstance)> for ForeignAssetInstance {
    fn from((asset_id, asset_instance): (XcmAssetId, XcmAssetInstance)) -> Self {
        Self {
            asset_id,
            asset_instance,
        }
    }
}

/// A categorized class instance represents either
/// a local class instance or a derivative class instance corresponding to a foreign one on a remote chain.
#[derive(Debug, PartialEq, Eq, Clone, Encode, Decode, MaxEncodedLen, TypeInfo)]
pub enum CategorizedClassInstance<LocalInstance, DerivativeInstance> {
    /// A local class instance.
    Local(LocalInstance),

    /// A derivative class instance corresponding to a foreign NFT on a remote chain.
    Derivative {
        /// The foreign asset instance to which the derivative corresponds.
        foreign_asset_instance: Box<ForeignAssetInstance>,

        /// The derivative class instance on this chain corresponding to the foreign one.
        derivative: DerivativeInstance,
    },
}
