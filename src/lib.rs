#![warn(missing_docs)]
#![cfg_attr(not(feature = "std"), no_std)]

//! The xnft pallet is a generalized NFT XCM Asset Transactor.
//! It can be integrated into any Substrate chain
//! containing an NFT pallet implementing the [`NftPallet`] trait.

use frame_support::{ensure, pallet_prelude::*, PalletId};
use frame_system::pallet_prelude::*;
use sp_runtime::{
    traits::{AccountIdConversion, BadOrigin},
    DispatchResult,
};
use sp_std::boxed::Box;
use xcm::{v3::prelude::*, VersionedAssetId};
use xcm_executor::traits::ConvertLocation;

use traits::NftPallet;

pub use pallet::*;

pub mod misc;
pub mod traits;

mod transact_asset;

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;

type CollectionIdOf<T> = <<T as Config>::NftPallet as NftPallet<T>>::CollectionId;
type TokenIdOf<T> = <<T as Config>::NftPallet as NftPallet<T>>::TokenId;
type LocationToAccountId<T> = <T as Config>::LocationToAccountId;

#[frame_support::pallet]
pub mod pallet {
    use traits::NftPallet;

    use super::*;

    #[pallet::config]
    pub trait Config: frame_system::Config {
        /// The aggregated event type of the runtime.
        type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

        /// The chain's Universal Location.
        type UniversalLocation: Get<InteriorMultiLocation>;

        /// The xnft pallet's ID.
        type PalletId: Get<PalletId>;

        /// The interior multilocation of all NFT collections on the chain.
        ///
        /// For instance, it could be the location of the chain's NFT pallet.
        /// This location serves as the prefix to the multilocation of a local NFT asset.
        type NftCollectionsLocation: Get<InteriorMultiLocation>;

        /// A converter from a multilocation to the chain's account ID.
        type LocationToAccountId: ConvertLocation<Self::AccountId>;

        /// A pallet that is capable of NFT operations.
        type NftPallet: NftPallet<Self>;

        /// An origin allowed to register foreign NFT collections.
        type RegisterOrigin: EnsureOrigin<
            Self::RuntimeOrigin,
            Success = ForeignCollectionAllowedToRegister,
        >;
    }

    /// Error for non-fungible-token module.
    #[pallet::error]
    pub enum Error<T> {
        /// The asset is already registered.
        AssetAlreadyRegistered,

        /// The given asset ID is not a foreign one.
        NotForeignAssetId,

        /// The given asset ID could not be converted into the current XCM version.
        BadAssetId,
    }

    #[pallet::event]
    #[pallet::generate_deposit(pub(crate) fn deposit_event)]
    pub enum Event<T: Config> {
        /// The given NFT asset (collection) is registered.
        AssetRegistered {
            /// The versioned XCM asset ID of the registered asset.
            asset_id: Box<VersionedAssetId>,

            /// The chain-local NFT collection ID of the registered asset.
            collection_id: CollectionIdOf<T>,
        },
    }

    #[pallet::origin]
    /// The xnft pallet's origin type.
    pub type Origin = XnftOrigin;

    #[pallet::storage]
    #[pallet::getter(fn foreign_asset_to_collection)]
    pub type ForeignAssetToCollection<T: Config> =
        StorageMap<_, Twox64Concat, xcm::v3::AssetId, CollectionIdOf<T>, OptionQuery>;

    #[pallet::storage]
    #[pallet::getter(fn collection_to_foreign_asset)]
    pub type CollectionToForeignAsset<T: Config> =
        StorageMap<_, Twox64Concat, CollectionIdOf<T>, xcm::v3::AssetId, OptionQuery>;

    #[pallet::storage]
    #[pallet::getter(fn foreign_instance_to_derivative_status)]
    pub type ForeignInstanceToDerivativeStatus<T: Config> = StorageDoubleMap<
        _,
        Twox64Concat,
        CollectionIdOf<T>,
        Blake2_128Concat,
        xcm::v3::AssetInstance,
        DerivativeTokenStatus<T>,
        OptionQuery,
    >;

    #[pallet::storage]
    #[pallet::getter(fn derivative_to_foreign_instance)]
    pub type DerivativeToForeignInstance<T: Config> = StorageDoubleMap<
        _,
        Twox64Concat,
        CollectionIdOf<T>,
        Blake2_128Concat,
        TokenIdOf<T>,
        xcm::v3::AssetInstance,
        OptionQuery,
    >;

    #[pallet::pallet]
    pub struct Pallet<T>(_);

    #[pallet::call]
    impl<T: Config> Pallet<T> {
        #[pallet::call_index(0)]
        #[pallet::weight(Weight::from_parts(1_000_000, 0)
			.saturating_add(T::DbWeight::get().reads(1))
			.saturating_add(T::DbWeight::get().writes(2)))]
        /// Register a derivative NFT collection.
        ///
        /// The collection will be backed by the reserve location
        /// identified by the `versioned_foreign_asset`.
        pub fn register_asset(
            origin: OriginFor<T>,
            versioned_foreign_asset: Box<VersionedAssetId>,
        ) -> DispatchResult {
            let allowed_asset_id = T::RegisterOrigin::ensure_origin(origin)?;

            let foreign_asset: AssetId = versioned_foreign_asset
                .as_ref()
                .clone()
                .try_into()
                .map_err(|()| Error::<T>::BadAssetId)?;

            let foreign_asset = Self::simplified_asset_id(foreign_asset);

            if let AssetId::Concrete(location) = foreign_asset {
                ensure!(location.parents > 0, <Error<T>>::NotForeignAssetId);
            }

            if let ForeignCollectionAllowedToRegister::Definite(allowed_asset_id) = allowed_asset_id
            {
                ensure!(foreign_asset == *allowed_asset_id, BadOrigin);
            }

            ensure!(
                !<ForeignAssetToCollection<T>>::contains_key(foreign_asset),
                <Error<T>>::AssetAlreadyRegistered,
            );

            let collection_id = T::NftPallet::create_derivative_collection(&Self::account_id())?;

            <ForeignAssetToCollection<T>>::insert(foreign_asset, collection_id.clone());
            <CollectionToForeignAsset<T>>::insert(collection_id.clone(), foreign_asset);

            Self::deposit_event(Event::AssetRegistered {
                asset_id: versioned_foreign_asset,
                collection_id,
            });

            Ok(())
        }
    }
}

impl<T: Config> Pallet<T> {
    /// The xnft pallet's account ID derived from the pallet ID.
    pub fn account_id() -> T::AccountId {
        <T as Config>::PalletId::get().into_account_truncating()
    }

    /// The collection's account ID. It is a sub-account of the xnft pallet account.
    pub fn collection_account_id(collection_id: CollectionIdOf<T>) -> Option<T::AccountId> {
        <T as Config>::PalletId::get().try_into_sub_account(collection_id)
    }

    fn simplified_asset_id(mut asset_id: AssetId) -> AssetId {
        if let AssetId::Concrete(location) = &mut asset_id {
            let context = T::UniversalLocation::get();
            location.simplify(&context);
        }

        asset_id
    }
}

/// An allowed XCM asset ID that can be registered
/// as a derivative NFT collection by the given origin.
pub enum ForeignCollectionAllowedToRegister {
    /// The given origin may register any derivative collection.
    Any,

    /// The given origin may register only the derivative collection
    /// backed by the definite foreign collection.
    Definite(Box<AssetId>),
}

#[derive(Debug, PartialEq, Eq, Clone, Encode, Decode, TypeInfo, MaxEncodedLen)]
/// The xnft pallet origin.
pub enum XnftOrigin {
    /// The origin of a foreign collection identified by the XCM asset ID.
    ForeignCollection(AssetId),
}

#[derive(Debug, PartialEq, Eq, Clone, Encode, Decode, MaxEncodedLen, TypeInfo)]
#[scale_info(skip_type_params(T))]
/// The status of a derivative token.
pub enum DerivativeTokenStatus<T: Config> {
    /// The given derivative is active,
    /// meaning it is backed by the original asset and owned by a user on this chain.
    Active(TokenIdOf<T>),

    /// The given derivative is stashed,
    /// meaning the original asset does not back it now, and no one on this chain can own it.
    ///
    /// This token will become active when
    /// the original asset is deposited into this chain again.
    Stashed(TokenIdOf<T>),
}

impl<T: Config> DerivativeTokenStatus<T> {
    fn token_id(self) -> TokenIdOf<T> {
        match self {
            Self::Active(id) => id,
            Self::Stashed(id) => id,
        }
    }
}
