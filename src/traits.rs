//! The traits are to be implemented by a Substrate chain where the xnft pallet is to be integrated.

use frame_support::{pallet_prelude::*, traits::PalletInfo};
use parity_scale_codec::{Decode, MaxEncodedLen};
use sp_runtime::{traits::MaybeEquivalence, DispatchError, ModuleError};
use xcm::v3::{prelude::*, Error as XcmError};

pub trait LocalAssetId: Member + Parameter + MaxEncodedLen {}
impl<T: Member + Parameter + MaxEncodedLen> LocalAssetId for T {}

pub trait LocalInstanceId: Member + Parameter + MaxEncodedLen {}
impl<T: Member + Parameter + MaxEncodedLen> LocalInstanceId for T {}

pub trait InteriorAssetIdConvert:
    MaybeEquivalence<InteriorMultiLocation, Self::LocalAssetId>
{
    type LocalAssetId: LocalAssetId;
}

pub trait InteriorAssetInstanceConvert:
    MaybeEquivalence<AssetInstance, Self::LocalInstanceId>
{
    type LocalInstanceId: LocalInstanceId;
}

pub type EngineAssetId<T, E> =
    <<E as NftEngine<T>>::InteriorAssetIdConvert as InteriorAssetIdConvert>::LocalAssetId;
pub type EngineInstanceIdOf<T, E> =
    <<E as NftEngine<T>>::InteriorAssetInstanceConvert as InteriorAssetInstanceConvert>::LocalInstanceId;

/// This trait describes the NFT Engine (i.e., an NFT solution) the chain implements.
///
/// NOTE: The transactionality of all of these operations
/// is governed by the XCM Executor's `TransactionalProcessor`.
/// See https://github.com/paritytech/polkadot-sdk/pull/1222.
pub trait NftEngine<T: frame_system::Config> {
    type InteriorAssetIdConvert: InteriorAssetIdConvert;

    type InteriorAssetInstanceConvert: InteriorAssetInstanceConvert;

    /// Pallet dispatch errors that are convertible to XCM errors.
    ///
    /// A type implementing [`IntoXcmError`], [`PalletError`], and [`Decode`] traits
    /// or a tuple constructed from such types can be used.
    ///
    /// This type allows the xnft pallet to decode certain pallet errors into proper XCM errors.
    ///
    /// The [`FailedToTransactAsset`](XcmError::FailedToTransactAsset) is a fallback
    /// when the dispatch error can't be decoded into any of the specified dispatch error types.
    type PalletDispatchErrors: DispatchErrorToXcmError<T>;

    /// Extra data which to be used to create a new derivative asset.
    type DerivativeAssetData: Member + Parameter;

    /// Asset creation weight.
    type AssetCreationWeight: AssetCreationWeight<Self::DerivativeAssetData>;

    /// Create a derivative NFT asset with the given `owner`.
    fn create_derivative_asset(
        owner: &T::AccountId,
        data: Self::DerivativeAssetData,
    ) -> Result<EngineAssetId<T, Self>, DispatchError>;

    /// Mint a new derivative NFT within the specified derivative asset to the `to` account.
    fn mint_derivative(
        asset_id: &EngineAssetId<T, Self>,
        to: &T::AccountId,
    ) -> Result<EngineInstanceIdOf<T, Self>, DispatchError>;

    /// Withdraw a derivative from the `from` account.
    ///
    /// The derivative can be either burned or stashed.
    /// The choice of what operation to use is up to the trait's implementation.
    ///
    /// * If the implementation has burned the derivative, it must return the [`DerivativeWithdrawal::Burned`] value.
    /// * If the implementation wants to stash the derivative, it should return the [`DerivativeWithdrawal::Stash`] value.
    fn withdraw_derivative(
        asset_id: &EngineAssetId<T, Self>,
        instance_id: &EngineInstanceIdOf<T, Self>,
        from: &T::AccountId,
    ) -> Result<DerivativeWithdrawal, DispatchError>;

    /// Transfer any local asset instance (derivative or local)
    /// from the `from` account to the `to` account
    fn transfer(
        asset_id: &EngineAssetId<T, Self>,
        instance_id: &EngineInstanceIdOf<T, Self>,
        from: &T::AccountId,
        to: &T::AccountId,
    ) -> DispatchResult;
}

/// Asset creation weight.
pub trait AssetCreationWeight<CreationData> {
    /// Compute the asset creation weight.
    fn asset_creation_weight(data: &CreationData) -> Weight;
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
