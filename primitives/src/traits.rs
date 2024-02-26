//! The traits are to be implemented by a Substrate chain where the xnft pallet is to be integrated.

use frame_support::pallet_prelude::*;
use parity_scale_codec::{Decode, MaxEncodedLen};
use sp_runtime::{DispatchError, ModuleError};
use xcm::latest::Error as XcmError;

/// This trait describes the NFT Engine (i.e., an NFT solution) the chain implements.
pub trait NftEngine<SystemAccountId> {
    /// The account ID type the engine uses.
    type AccountId: Parameter + Member + MaxEncodedLen;

    /// The class type provides the ID type for classes
    /// and the interface to create new classes.
    type Class: NftClass<Self::AccountId>;

    /// The ID type for class instances.
    type ClassInstanceId: Member + Parameter + MaxEncodedLen;

    /// Transfer any local class instance (derivative or local)
    /// from the `from` account to the `to` account
    fn transfer_class_instance(
        class_id: &<Self::Class as NftClass<Self::AccountId>>::ClassId,
        instance_id: &Self::ClassInstanceId,
        from: &Self::AccountId,
        to: &Self::AccountId,
    ) -> DispatchResult;

    /// Mint a new derivative NFT within the specified derivative class to the `to` account.
    fn mint_derivative(
        class_id: &<Self::Class as NftClass<Self::AccountId>>::ClassId,
        to: &Self::AccountId,
    ) -> Result<Self::ClassInstanceId, DispatchError>;

    /// Withdraw a derivative from the `from` account.
    ///
    /// The derivative can be either burned or stashed.
    /// The choice of what operation to use is up to the trait's implementation.
    ///
    /// * If the implementation has burned the derivative, it must return the [`DerivativeWithdrawal::Burned`] value.
    /// * If the implementation wants to stash the derivative, it should return the [`DerivativeWithdrawal::Stash`] value.
    fn withdraw_derivative(
        class_id: &<Self::Class as NftClass<Self::AccountId>>::ClassId,
        instance_id: &Self::ClassInstanceId,
        from: &Self::AccountId,
    ) -> Result<DerivativeWithdrawal, DispatchError>;
}

/// The class type provides the ID type for classes
/// and the interface to create new classes.
pub trait NftClass<AccountId> {
    /// The ID type for classes.
    type ClassId: Member + Parameter + MaxEncodedLen;

    /// Extra data which to be used to create a new class.
    type ClassData: Member + Parameter;

    /// Compute the class creation weight.
    fn create_class_weight(data: &Self::ClassData) -> Weight;

    /// Create a new class.
    fn create_class(
        owner: &AccountId,
        data: Self::ClassData,
    ) -> Result<Self::ClassId, DispatchError>;
}

/// Derivative withdrawal operation.
pub enum DerivativeWithdrawal {
    /// Indicate that the derivative is burned.
    Burned,

    /// Indicate that the derivative should be stashed.
    Stash,
}

/// The conversion from a pallet error to the [`XcmError`].
pub trait DispatchErrorConvert {
    /// The Pallet to which the error belongs.
    type Pallet: 'static;

    /// The Error type.
    type Error: Decode;

    /// Converts the `Pallet`'s `Error` into the [`XcmError`].
    fn convert(error: Self::Error) -> XcmError;
}

/// The conversion from the [`DispatchError`] to the [`XcmError`].
pub trait DispatchErrorsConvert<T: frame_system::Config> {
    /// Convert the `error` into the [`XcmError`].
    fn convert(error: DispatchError) -> XcmError;
}

macro_rules! impl_to_xcm_error {
	($($gen:ident)*) => {
        impl<T, $($gen,)*> $crate::traits::DispatchErrorsConvert<T> for ($($gen,)*)
        where
            T: frame_system::Config,
            $($gen: $crate::traits::DispatchErrorConvert,)*
        {
            fn convert(error: sp_runtime::DispatchError) -> xcm::latest::Error {
                use xcm::latest::Error;

                #[allow(unused)]
                use frame_support::traits::PalletInfo;

                #[allow(unused)]
                use $crate::traits::DispatchErrorConvert;

                match error {
                    #[allow(unused_variables)]
                    DispatchError::Module(ModuleError {
                        index,
                        error,
                        message,
                    }) => {
                        $(
                            if let Some(err_idx) = T::PalletInfo::index::<$gen::Pallet>() {
                                if index as usize == err_idx {
                                    let mut read = &error as &[u8];
                                    match <$gen as DispatchErrorConvert>::Error::decode(&mut read) {
                                        Ok(error) => return $gen::convert(error),
                                        Err(_) => return Error::FailedToTransactAsset(
                                            "Failed to decode a module error"
                                        ),
                                    }
                                }
                            }
                        )*

                        Error::FailedToTransactAsset(message.unwrap_or("Unknown module error"))
                    },
                    DispatchError::BadOrigin => Error::BadOrigin,
                    _ => Error::FailedToTransactAsset(error.into()),
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
    @ A B C D E F G H I J K L M N O P
}

impl<T: frame_system::Config, E: DispatchErrorConvert> DispatchErrorsConvert<T> for E {
    fn convert(error: DispatchError) -> XcmError {
        <(E,) as DispatchErrorsConvert<T>>::convert(error)
    }
}
