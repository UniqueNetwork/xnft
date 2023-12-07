use frame_support::{pallet_prelude::*, traits::PalletInfo};
use parity_scale_codec::{Decode, MaxEncodedLen};
use sp_runtime::{DispatchError, ModuleError};
use xcm::v3::{prelude::*, Error as XcmError};

pub trait NftPallet<T: frame_system::Config> {
    type CollectionId: Member + Parameter + MaxEncodedLen + TryFrom<Junction>;
    type TokenId: Member + Parameter + MaxEncodedLen + TryFrom<AssetInstance>;

    type PalletDispatchErrors: DispatchErrorToXcmError<T>;

    fn create_derivative_collection(
        owner: &T::AccountId,
    ) -> Result<Self::CollectionId, DispatchError>;

    fn deposit_derivative(
        collection_id: &Self::CollectionId,
        stahed_token_id: Option<&Self::TokenId>,
        to: &T::AccountId,
    ) -> Result<Self::TokenId, DispatchError>;

    fn withdraw_derivative(
        collection_id: &Self::CollectionId,
        token_id: &Self::TokenId,
        from: &T::AccountId,
    ) -> Result<DerivativeWithdrawResult, DispatchError>;

    fn transfer(
        collection_id: &Self::CollectionId,
        token_id: &Self::TokenId,
        from: &T::AccountId,
        to: &T::AccountId,
    ) -> DispatchResult;
}

pub trait PalletError {
    type Pallet: 'static;
}

pub trait IntoXcmError {
    fn into_xcm_error(self) -> XcmError;
}

pub trait DispatchErrorToXcmError<T: frame_system::Config> {
    fn to_xcm_error(error: DispatchError) -> XcmError;
}

macro_rules! impl_to_xcm_error {
	($($gen:ident)*) => {
        impl<T, $($gen,)*> DispatchErrorToXcmError<T> for ($($gen,)*)
        where
            T: frame_system::Config,
            $($gen: PalletError + IntoXcmError + Decode,)*
        {
            fn to_xcm_error(error: DispatchError) -> XcmError {
                match error {
                    DispatchError::Module(ModuleError {
                        index,
                        error,
                        message,
                    }) => {
                        $(
                            if let Some(err_idx) = <T::PalletInfo as PalletInfo>::index::<$gen::Pallet>() {
                                if index as usize == err_idx {
                                    let mut read = &error as &[u8];
                                    match $gen::decode(&mut read) {
                                        Ok(error) => return error.into_xcm_error(),
                                        Err(_) => return XcmError::FailedToTransactAsset(
                                            "Failed to decode a module error"
                                        ),
                                    }
                                }
                            }
                        )*

                        XcmError::FailedToTransactAsset(message.unwrap_or("Unknown module error"))
                    },
                    DispatchError::BadOrigin => XcmError::BadOrigin,
                    _ => XcmError::FailedToTransactAsset(error.into()),
                }
            }
        }
	};
	($($cur:ident)* @ $c:ident $($rest:ident)*) => {
		impl_to_xcm_error!($($cur)*);
		impl_to_xcm_error!($($cur)* $c @ $($rest)*);
	};
	($($cur:ident)* @) => {
		impl_to_xcm_error!($($cur)*);
	}
}
impl_to_xcm_error! {
    A @ B C D E F G H I J K L M N O P
}

impl<T: frame_system::Config> DispatchErrorToXcmError<T> for () {
    fn to_xcm_error(error: DispatchError) -> XcmError {
        match error {
            DispatchError::BadOrigin => XcmError::BadOrigin,
            _ => XcmError::FailedToTransactAsset(error.into()),
        }
    }
}

impl<T: frame_system::Config, E: PalletError + IntoXcmError + Decode> DispatchErrorToXcmError<T>
    for E
{
    fn to_xcm_error(error: DispatchError) -> XcmError {
        <(E,) as DispatchErrorToXcmError<T>>::to_xcm_error(error)
    }
}

pub enum DerivativeWithdrawResult {
    Burned,
    Stashed,
}
