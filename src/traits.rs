//! The traits are to be implemented by a Substrate chain where the xnft pallet is to be integrated.

use core::fmt::Debug;

use frame_support::{pallet_prelude::*, traits::PalletInfo};
use parity_scale_codec::{Decode, MaxEncodedLen};
use sp_runtime::{DispatchError, ModuleError};
use xcm::v3::Error as XcmError;

pub trait ClassId: Member + Parameter + MaxEncodedLen {}
impl<T: Member + Parameter + MaxEncodedLen> ClassId for T {}

pub trait ClassInstanceId: Member + Parameter + MaxEncodedLen {}
impl<T: Member + Parameter + MaxEncodedLen> ClassInstanceId for T {}

/// This trait describes the NFT Engine (i.e., an NFT solution) the chain implements.
///
/// NOTE: The transactionality of all of these operations
/// is governed by the XCM Executor's `TransactionalProcessor`.
/// See https://github.com/paritytech/polkadot-sdk/pull/1222.
pub trait NftEngine {
    type AccountId: Debug + Clone + PartialEq + Eq;

    /// The class ID type.
    type ClassId: ClassId;

    /// The class instance ID type.
    type ClassInstanceId: ClassInstanceId;

    type DerivativeClassCreation: DerivativeClassCreation<Self::ClassId>;

    /// Transfer any local class instance (derivative or local)
    /// from the `from` account to the `to` account
    fn transfer_class_instance(
        class_id: &Self::ClassId,
        instance_id: &Self::ClassInstanceId,
        from: &Self::AccountId,
        to: &Self::AccountId,
    ) -> DispatchResult;

    fn stash_account_id() -> Self::AccountId;

    /// Mint a new derivative NFT within the specified derivative class to the `to` account.
    fn mint_derivative(
        class_id: &Self::ClassId,
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
        class_id: &Self::ClassId,
        instance_id: &Self::ClassInstanceId,
        from: &Self::AccountId,
    ) -> Result<DerivativeWithdrawal, DispatchError>;
}

pub trait DerivativeClassCreation<ClassId> {
    /// Extra data which to be used to create a new derivative class.
    type DerivativeClassData: Member + Parameter;

    /// Compute the class creation weight.
    fn class_creation_weight(data: &Self::DerivativeClassData) -> Weight;

    /// Create a new derivative class.
    fn create_derivative_class(data: Self::DerivativeClassData) -> Result<ClassId, DispatchError>;
}

/// Derivative withdrawal operation.
pub enum DerivativeWithdrawal {
    /// Indicate that the derivative is burned.
    Burned,

    /// Indicate that the derivative should be stashed.
    Stash,
}

/// The implementation of this trait is an error
/// of the pallet identified by the corresponding associated type.
pub trait PalletError {
    /// The pallet to which the error belongs.
    type Pallet: 'static;
}

/// The conversion to the [`XcmError`].
pub trait IntoXcmError {
    /// Convert the value into the [`XcmError`].
    fn into_xcm_error(self) -> XcmError;
}

/// The conversion from the [`DispatchError`] to the [`XcmError`].
pub trait DispatchErrorToXcmError<T: frame_system::Config> {
    /// Convert the `error` into the [`XcmError`].
    fn dispatch_error_to_xcm_error(error: DispatchError) -> XcmError;
}

macro_rules! impl_to_xcm_error {
	($($gen:ident)*) => {
        impl<T, $($gen,)*> DispatchErrorToXcmError<T> for ($($gen,)*)
        where
            T: frame_system::Config,
            $($gen: PalletError + IntoXcmError + Decode,)*
        {
            fn dispatch_error_to_xcm_error(error: DispatchError) -> XcmError {
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
    fn dispatch_error_to_xcm_error(error: DispatchError) -> XcmError {
        match error {
            DispatchError::BadOrigin => XcmError::BadOrigin,
            _ => XcmError::FailedToTransactAsset(error.into()),
        }
    }
}

impl<T: frame_system::Config, E: PalletError + IntoXcmError + Decode> DispatchErrorToXcmError<T>
    for E
{
    fn dispatch_error_to_xcm_error(error: DispatchError) -> XcmError {
        <(E,) as DispatchErrorToXcmError<T>>::dispatch_error_to_xcm_error(error)
    }
}
