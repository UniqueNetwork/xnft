use core::marker::PhantomData;
use core::ops::Deref;

use sp_std::vec;

use frame_support::{pallet_prelude::DispatchResult, Parameter};
use parity_scale_codec::MaxEncodedLen;
use sp_runtime::{
    traits::{Get, Member},
    DispatchError,
};
use xcm::v3::prelude::*;

use crate::traits::{DerivativeWithdrawResult, IntoXcmError, NftPallet, PalletError};

use orml_nft::{Config as OrmlNftConfig, Error as OrmlNftError, Pallet as OrmlNftPallet};

pub struct OrmlXnftAdapter<T, CollectionId, TokenId, DerivativeClassData, DerivativeTokenData>(
    PhantomData<(
        T,
        CollectionId,
        TokenId,
        DerivativeClassData,
        DerivativeTokenData,
    )>,
)
where
    T: OrmlNftConfig,
    CollectionId: Deref<Target = T::ClassId>
        + From<T::ClassId>
        + Member
        + Parameter
        + MaxEncodedLen
        + TryFrom<Junction>,
    TokenId: Deref<Target = T::TokenId>
        + From<T::TokenId>
        + Member
        + Parameter
        + MaxEncodedLen
        + TryFrom<AssetInstance>,
    DerivativeClassData: Get<T::ClassData>,
    DerivativeTokenData: Get<T::TokenData>;

impl<T, CollectionId, TokenId, DerivativeClassData, DerivativeTokenData> NftPallet<T>
    for OrmlXnftAdapter<T, CollectionId, TokenId, DerivativeClassData, DerivativeTokenData>
where
    T: OrmlNftConfig,
    CollectionId: Deref<Target = T::ClassId>
        + From<T::ClassId>
        + Member
        + Parameter
        + MaxEncodedLen
        + TryFrom<Junction>,
    TokenId: Deref<Target = T::TokenId>
        + From<T::TokenId>
        + Member
        + Parameter
        + MaxEncodedLen
        + TryFrom<AssetInstance>,
    DerivativeClassData: Get<T::ClassData>,
    DerivativeTokenData: Get<T::TokenData>,
{
    type CollectionId = CollectionId;
    type TokenId = TokenId;
    type PalletDispatchErrors = OrmlNftError<T>;

    fn create_derivative_collection(
        owner: &T::AccountId,
    ) -> Result<Self::CollectionId, DispatchError> {
        <OrmlNftPallet<T>>::create_class(owner, vec![], DerivativeClassData::get()).map(Into::into)
    }

    fn deposit_derivative(
        collection_id: &Self::CollectionId,
        _stahed_token_id: Option<&Self::TokenId>,
        to: &<T as frame_system::Config>::AccountId,
    ) -> Result<Self::TokenId, DispatchError> {
        <OrmlNftPallet<T>>::mint(
            to,
            *collection_id.clone(),
            vec![],
            DerivativeTokenData::get(),
        )
        .map(Into::into)
    }

    fn withdraw_derivative(
        collection_id: &Self::CollectionId,
        token_id: &Self::TokenId,
        from: &T::AccountId,
    ) -> Result<DerivativeWithdrawResult, DispatchError> {
        <OrmlNftPallet<T>>::burn(from, (*collection_id.deref(), *token_id.deref()))
            .map(|()| DerivativeWithdrawResult::Burned)
    }

    fn transfer(
        collection_id: &Self::CollectionId,
        token_id: &Self::TokenId,
        from: &T::AccountId,
        to: &T::AccountId,
    ) -> DispatchResult {
        <OrmlNftPallet<T>>::transfer(from, to, (*collection_id.deref(), *token_id.deref()))
    }
}

impl<T: OrmlNftConfig> PalletError for OrmlNftError<T> {
    type Pallet = OrmlNftPallet<T>;
}
impl<T: OrmlNftConfig> IntoXcmError for OrmlNftError<T> {
    fn into_xcm_error(self) -> XcmError {
        match self {
            OrmlNftError::ClassNotFound => XcmError::AssetNotFound,
            OrmlNftError::NoPermission => XcmError::NoPermission,
            _ => XcmError::FailedToTransactAsset(self.into()),
        }
    }
}
