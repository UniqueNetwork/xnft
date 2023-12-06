use frame_support::{ensure, pallet_prelude::*, PalletId};
use frame_system::pallet_prelude::*;
use sp_runtime::{
    traits::{AccountIdConversion, BadOrigin},
    DispatchResult,
};
use xcm::{
    v3::{AssetId, InteriorMultiLocation, MultiAsset, MultiLocation},
    VersionedAssetId,
};
use xcm_executor::traits::ConvertLocation;

use traits::NftPallet;

pub use pallet::*;

pub mod misc;
pub mod traits;
mod transact_asset;

pub type CollectionIdOf<T> = <<T as Config>::NftPallet as NftPallet<T>>::CollectionId;
pub type TokenIdOf<T> = <<T as Config>::NftPallet as NftPallet<T>>::TokenId;
pub type LocationToAccountId<T> = <T as Config>::LocationToAccountId;

pub enum ForeignCollectionAllowedToRegister {
    Any,
    Definite(AssetId),
}

pub enum RawOrigin {
    ForeignCollection(AssetId),
}

#[derive(Debug, PartialEq, Eq, Clone, Encode, Decode, MaxEncodedLen, TypeInfo)]
#[scale_info(skip_type_params(T))]
pub enum DerivativeTokenStatus<T: Config> {
    Active(TokenIdOf<T>),
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

#[frame_support::pallet]
pub mod pallet {
    use traits::NftPallet;

    use super::*;

    #[pallet::config]
    pub trait Config: frame_system::Config {
        type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

        /// This chain's Universal Location.
        type UniversalLocation: Get<InteriorMultiLocation>;

        type PalletId: Get<PalletId>;

        /// TODO proper doc
        /// Could be native NFT pallet location.
        type NftCollectionsLocation: Get<InteriorMultiLocation>;

        type LocationToAccountId: ConvertLocation<Self::AccountId>;

        type NftPallet: NftPallet<Self>;

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
        AssetRegistered {
            asset_id: Box<VersionedAssetId>,
            collection_id: CollectionIdOf<T>,
        },
    }

    #[pallet::origin]
    pub type Origin = RawOrigin;

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
                ensure!(foreign_asset == allowed_asset_id, BadOrigin);
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
    pub fn account_id() -> T::AccountId {
        <T as Config>::PalletId::get().into_account_truncating()
    }

    pub fn collection_account_id(collection_id: CollectionIdOf<T>) -> Option<T::AccountId> {
        <T as Config>::PalletId::get().try_into_sub_account(collection_id)
    }

    fn simplified_location(mut location: MultiLocation) -> MultiLocation {
        let context = T::UniversalLocation::get();
        location.simplify(&context);
        location
    }

    fn simplified_asset_id(asset_id: AssetId) -> AssetId {
        match asset_id {
            AssetId::Concrete(location) => AssetId::Concrete(Self::simplified_location(location)),
            _ => asset_id,
        }
    }

    fn simplified_multiasset(mut multiasset: MultiAsset) -> MultiAsset {
        multiasset.id = Self::simplified_asset_id(multiasset.id);
        multiasset
    }
}
