#![warn(missing_docs)]
#![cfg_attr(not(feature = "std"), no_std)]

//! The xnft pallet is a generalized NFT XCM Asset Transactor.
//! It can be integrated into any Substrate chain implementing the [`NftInterface`] trait.

use frame_support::{ensure, pallet_prelude::*, traits::EnsureOriginWithArg, PalletId};
use frame_system::pallet_prelude::*;
use sp_runtime::{traits::AccountIdConversion, DispatchResult};
use sp_std::boxed::Box;
use xcm::{v3::prelude::*, VersionedAssetId};
use xcm_executor::traits::{ConvertLocation, Error as XcmExecutorError};

use traits::{CollectionCreationWeight, NftInterface};

pub use pallet::*;

pub mod misc;
pub mod traits;

#[allow(missing_docs)]
pub mod weights;

mod transact_asset;

#[cfg(test)]
mod tests;

#[cfg(feature = "runtime-benchmarks")]
#[allow(missing_docs)]
pub mod benchmarking;

type CollectionIdOf<T, I> = <<T as Config<I>>::NftInterface as NftInterface<T>>::CollectionId;
type TokenIdOf<T, I> = <<T as Config<I>>::NftInterface as NftInterface<T>>::TokenId;
type LocationToAccountIdOf<T, I> = <T as Config<I>>::LocationToAccountId;
type CollectionCreationWeightOf<T, I> =
    <<T as Config<I>>::NftInterface as NftInterface<T>>::CollectionCreationWeight;

#[frame_support::pallet]
pub mod pallet {
    use weights::WeightInfo;

    use super::*;

    #[pallet::config]
    pub trait Config<I: 'static = ()>: frame_system::Config {
        /// The aggregated event type of the runtime.
        type RuntimeEvent: From<Event<Self, I>>
            + IsType<<Self as frame_system::Config>::RuntimeEvent>;

        /// The weight info.
        type WeightInfo: WeightInfo;

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
        type RegisterOrigin: EnsureOriginWithArg<Self::RuntimeOrigin, AssetId>;
    }

    /// Error for non-fungible-token module.
    #[pallet::error]
    pub enum Error<T, I = ()> {
        /// The asset is already registered.
        AssetAlreadyRegistered,

        /// The given asset ID is not a foreign one.
        NotForeignAssetId,

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

            /// The derivative collection ID of the registered asset.
            derivative_collection_id: CollectionIdOf<T, I>,
        },

        /// A token is deposited.
        Deposited {
            /// The token in question.
            token: CategorizedToken<NativeTokenOf<T, I>, NativeTokenOf<T, I>>,

            /// The account to whom the NFT derivative is deposited.
            to: T::AccountId,
        },

        /// A token is withdrawn.
        Withdrawn {
            /// The token in question.
            token: CategorizedToken<NativeTokenOf<T, I>, NativeTokenOf<T, I>>,

            /// The account from whom the NFT derivative is withdrawn.
            from: T::AccountId,
        },

        /// A token is transferred.
        Transferred {
            /// The token in question.
            token: CategorizedToken<NativeTokenOf<T, I>, NativeTokenOf<T, I>>,

            /// The account from whom the NFT derivative is withdrawn.
            from: T::AccountId,

            /// The account to whom the NFT derivative is deposited.
            to: T::AccountId,
        },
    }

    #[pallet::storage]
    #[pallet::getter(fn foreign_asset_to_collection)]
    pub type ForeignAssetToCollection<T: Config<I>, I: 'static = ()> =
        StorageMap<_, Blake2_128Concat, xcm::v3::AssetId, CollectionIdOf<T, I>, OptionQuery>;

    #[pallet::storage]
    #[pallet::getter(fn collection_to_foreign_asset)]
    pub type CollectionToForeignAsset<T: Config<I>, I: 'static = ()> =
        StorageMap<_, Blake2_128Concat, CollectionIdOf<T, I>, xcm::v3::AssetId, OptionQuery>;

    #[pallet::storage]
    #[pallet::getter(fn foreign_instance_to_derivative_status)]
    pub type ForeignInstanceToDerivativeIdStatus<T: Config<I>, I: 'static = ()> = StorageDoubleMap<
        _,
        Blake2_128Concat,
        CollectionIdOf<T, I>,
        Blake2_128Concat,
        xcm::v3::AssetInstance,
        DerivativeIdStatus<TokenIdOf<T, I>>,
        ValueQuery,
    >;

    #[pallet::storage]
    #[pallet::getter(fn derivative_to_foreign_instance)]
    pub type DerivativeIdToForeignInstance<T: Config<I>, I: 'static = ()> = StorageDoubleMap<
        _,
        Blake2_128Concat,
        CollectionIdOf<T, I>,
        Blake2_128Concat,
        TokenIdOf<T, I>,
        xcm::v3::AssetInstance,
        OptionQuery,
    >;

    #[pallet::pallet]
    pub struct Pallet<T, I = ()>(_);

    #[pallet::call]
    impl<T: Config<I>, I: 'static> Pallet<T, I> {
        /// Registers a foreign non-fungible asset.
        ///
        /// Creates a derivative collection on this chain
        /// backed by the foreign asset identified by the `versioned_foreign_asset`.
        #[pallet::call_index(0)]
        #[pallet::weight(T::WeightInfo::foreign_asset_registration_checks()
            .saturating_add(CollectionCreationWeightOf::<T, I>::collection_creation_weight(derivative_collection_data))
			.saturating_add(T::DbWeight::get().writes(3)))]
        pub fn register_foreign_asset(
            origin: OriginFor<T>,
            versioned_foreign_asset: Box<VersionedAssetId>,
            derivative_collection_data: <T::NftInterface as NftInterface<T>>::DerivativeCollectionData,
        ) -> DispatchResult {
            let foreign_asset_id =
                Self::foreign_asset_registration_checks(origin, versioned_foreign_asset)?;

            let derivative_collection_id = T::NftInterface::create_derivative_collection(
                &Self::account_id(),
                derivative_collection_data,
            )?;

            <ForeignAssetToCollection<T, I>>::insert(foreign_asset_id, &derivative_collection_id);
            <CollectionToForeignAsset<T, I>>::insert(&derivative_collection_id, foreign_asset_id);

            Self::deposit_event(Event::ForeignAssetRegistered {
                foreign_asset_id: Box::new(foreign_asset_id),
                derivative_collection_id,
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
            ensure!(location.parents > 0, <Error<T, I>>::NotForeignAssetId);
        }

        T::RegisterOrigin::ensure_origin(origin, &normalized_asset)?;

        ensure!(
            !<ForeignAssetToCollection<T, I>>::contains_key(normalized_asset),
            <Error<T, I>>::AssetAlreadyRegistered,
        );

        Ok(normalized_asset)
    }
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
    fn ensure_active(self) -> Result<TokenId, XcmError> {
        match self {
            Self::Active(id) => Ok(id),
            Self::Stashed(_) => Err(XcmError::NoPermission),
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
type NativeTokenOf<T, I> = Token<CollectionIdOf<T, I>, TokenIdOf<T, I>>;
