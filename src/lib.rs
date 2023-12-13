#![warn(missing_docs)]
#![cfg_attr(not(feature = "std"), no_std)]

//! The xnft pallet is a generalized NFT XCM Asset Transactor.
//! It can be integrated into any Substrate chain implementing the [`NftInterface`] trait.

use frame_support::{ensure, pallet_prelude::*, PalletId};
use frame_system::pallet_prelude::*;
use sp_runtime::{
    traits::{AccountIdConversion, BadOrigin},
    DispatchResult,
};
use sp_std::boxed::Box;
use xcm::{v3::prelude::*, VersionedAssetId};
use xcm_executor::traits::{ConvertLocation, Error as XcmExecutorError};

use traits::NftInterface;

pub use pallet::*;

pub mod misc;
pub mod traits;

mod transact_asset;

type CollectionIdOf<T> = <<T as Config>::NftInterface as NftInterface<T>>::CollectionId;
type TokenIdOf<T> = <<T as Config>::NftInterface as NftInterface<T>>::TokenId;
type LocationToAccountId<T> = <T as Config>::LocationToAccountId;

#[frame_support::pallet]
pub mod pallet {
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

        /// An implementation of the NFT interface.
        type NftInterface: NftInterface<Self>;

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

        /// A foreign NFT is deposited.
        Deposited {
            /// The token in question.
            token: CategorizedToken<NativeTokenOf<T>, NativeTokenOf<T>>,

            /// The account to whom the NFT derivative is deposited.
            beneficiary: T::AccountId,
        },

        /// A foreign NFT is withdrawn.
        Withdrawn {
            /// The token in question.
            token: CategorizedToken<NativeTokenOf<T>, NativeTokenOf<T>>,

            /// The account from whom the NFT derivative is withdrawn.
            benefactor: T::AccountId,
        },

        /// A foreign NFT is transferred.
        Transferred {
            /// The token in question.
            token: CategorizedToken<NativeTokenOf<T>, NativeTokenOf<T>>,

            /// The account from whom the NFT derivative is withdrawn.
            from: T::AccountId,

            /// The account to whom the NFT derivative is deposited.
            to: T::AccountId,
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
    pub type ForeignInstanceToDerivativeIdStatus<T: Config> = StorageDoubleMap<
        _,
        Twox64Concat,
        CollectionIdOf<T>,
        Blake2_128Concat,
        xcm::v3::AssetInstance,
        DerivativeIdStatus<TokenIdOf<T>>,
        ValueQuery,
    >;

    #[pallet::storage]
    #[pallet::getter(fn derivative_to_foreign_instance)]
    pub type DerivativeIdToForeignInstance<T: Config> = StorageDoubleMap<
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
        /// The collection will be backed by the foreign asset identified by the `versioned_foreign_asset`.
        pub fn register_asset(
            origin: OriginFor<T>,
            versioned_foreign_asset: Box<VersionedAssetId>,
            derivative_collection_data: <T::NftInterface as NftInterface<T>>::DerivativeCollectionData,
        ) -> DispatchResult {
            let allowed_asset_id = T::RegisterOrigin::ensure_origin(origin)?;

            let foreign_asset: AssetId = versioned_foreign_asset
                .as_ref()
                .clone()
                .try_into()
                .map_err(|()| Error::<T>::BadAssetId)?;

            let foreign_asset = Self::normalize_if_local_asset(foreign_asset);

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

            let collection_id = T::NftInterface::create_derivative_collection(
                &Self::account_id(),
                derivative_collection_data,
            )?;

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
    fn normalize_if_local_asset(mut asset_id: AssetId) -> AssetId {
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

#[derive(Default, Debug, PartialEq, Eq, Clone, Encode, Decode, MaxEncodedLen, TypeInfo)]
/// The status of a derivative token ID.
pub enum DerivativeIdStatus<TokenId> {
    /// The given derivative ID is active,
    /// meaning it is backed by the original asset and owned by a user on this chain.
    Active(TokenId),

    /// The given derivative ID is stashed,
    /// meaning the original asset does not back it now,
    /// and no one on this chain can own this derivative.
    ///
    /// This token ID will become active when the original asset is deposited into this chain again.
    Stashed(TokenId),

    /// No derivative ID exists.
    #[default]
    NotExists,
}

impl<TokenId> DerivativeIdStatus<TokenId> {
    fn existing(self) -> Result<TokenId, XcmError> {
        match self {
            Self::Active(id) => Ok(id),
            Self::Stashed(id) => Ok(id),
            Self::NotExists => Err(XcmExecutorError::InstanceConversionFailed.into()),
        }
    }
}

/// An NFT complete identification.
#[derive(Debug, PartialEq, Eq, Clone, Encode, Decode, MaxEncodedLen, TypeInfo)]
pub struct Token<CollectionId, TokenId> {
    /// The collection ID of the token.
    pub collection_id: CollectionId,

    /// The token's ID within the collection.
    pub token_id: TokenId,
}

impl<CollectionId, TokenId> From<(CollectionId, TokenId)> for Token<CollectionId, TokenId> {
    fn from((collection_id, token_id): (CollectionId, TokenId)) -> Self {
        Self {
            collection_id,
            token_id,
        }
    }
}

/// A categorized token represents either
/// a local token or a derivative token corresponding to a foreign token on a remote chain.
#[derive(Debug, PartialEq, Eq, Clone, Encode, Decode, MaxEncodedLen, TypeInfo)]
pub enum CategorizedToken<LocalToken, DerivativeToken> {
    /// A local token.
    Local(LocalToken),

    /// A derivative token corresponding to a foreign NFT on a remote chain.
    Derivative {
        /// The foreign token to which the derivative corresponds.
        foreign_token: ForeignToken,

        /// The derivative token on this chain corresponding to the foreign token.
        derivative_token: DerivativeToken,
    },
}

type ForeignToken = Token<Box<AssetId>, Box<AssetInstance>>;
type NativeTokenOf<T> = Token<CollectionIdOf<T>, TokenIdOf<T>>;
