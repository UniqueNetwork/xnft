use cumulus_pallet_parachain_system::AnyRelayNumber;
use cumulus_primitives_core::ParaId;
use frame_support::{
    construct_runtime, parameter_types,
    traits::{Everything, Nothing},
};
use frame_system::EnsureRoot;
use polkadot_runtime_common::xcm_sender::NoPriceForMessageDelivery;
use sp_core::{ConstU128, ConstU32, ConstU64, H256};
use sp_runtime::{traits::IdentityLookup, AccountId32};
use xcm::prelude::*;
use xcm_builder::{
    AllowTopLevelPaidExecutionFrom, EnsureXcmOrigin, FixedWeightBounds, SignedToAccountId32,
    TakeWeightCredit,
};
use xcm_executor::{
    traits::{TransactAsset, WeightTrader},
    Assets, XcmExecutor,
};

pub type Balance = u128;
pub type AccountId = AccountId32;

impl frame_system::Config for Runtime {
    type RuntimeOrigin = RuntimeOrigin;
    type RuntimeCall = RuntimeCall;
    type Nonce = u64;
    type Hash = H256;
    type Hashing = ::sp_runtime::traits::BlakeTwo256;
    type AccountId = AccountId;
    type Lookup = IdentityLookup<Self::AccountId>;
    type Block = Block;
    type RuntimeEvent = RuntimeEvent;
    type BlockHashCount = ConstU64<250>;
    type BlockWeights = ();
    type BlockLength = ();
    type Version = ();
    type PalletInfo = PalletInfo;
    type AccountData = pallet_balances::AccountData<Balance>;
    type OnNewAccount = ();
    type OnKilledAccount = ();
    type DbWeight = ();
    type BaseCallFilter = Everything;
    type SystemWeightInfo = ();
    type SS58Prefix = ();
    type OnSetCode = cumulus_pallet_parachain_system::ParachainSetCode<Runtime>;
    type MaxConsumers = ConstU32<16>;
}

impl pallet_balances::Config for Runtime {
    type MaxLocks = ConstU32<50>;
    type Balance = Balance;
    type RuntimeEvent = RuntimeEvent;
    type DustRemoval = ();
    type ExistentialDeposit = ConstU128<1>;
    type AccountStore = System;
    type WeightInfo = ();
    type MaxReserves = ConstU32<50>;
    type ReserveIdentifier = [u8; 8];
    type RuntimeHoldReason = RuntimeHoldReason;
    type RuntimeFreezeReason = RuntimeFreezeReason;
    type FreezeIdentifier = [u8; 8];
    type MaxHolds = ();
    type MaxFreezes = ();
}

impl parachain_info::Config for Runtime {}

impl cumulus_pallet_parachain_system::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type OnSystemEvent = ();
    type SelfParaId = ();
    type OutboundXcmpMessageSource = XcmpQueue;
    type DmpMessageHandler = ();
    type ReservedDmpWeight = ();
    type XcmpMessageHandler = XcmpQueue;
    type ReservedXcmpWeight = ();
    type CheckAssociatedRelayNumber = AnyRelayNumber;
}

impl cumulus_pallet_xcmp_queue::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type XcmExecutor = XcmExecutor<XcmConfig>;
    type ChannelInfo = ParachainSystem;
    type VersionWrapper = ();
    type ExecuteOverweightOrigin = EnsureRoot<AccountId>;
    type ControllerOrigin = EnsureRoot<AccountId>;
    type ControllerOriginConverter = ();
    type WeightInfo = ();
    type PriceForSiblingDelivery = NoPriceForMessageDelivery<ParaId>;
}

pub struct XcmConfig;
impl xcm_executor::Config for XcmConfig {
    type RuntimeCall = RuntimeCall;
    type XcmSender = XcmRouter;
    type AssetTransactor = DummyAssetTransactor;
    type OriginConverter = ();
    type IsReserve = ();
    type IsTeleporter = ();
    type UniversalLocation = UniversalLocation;
    type Barrier = Barrier;
    type Weigher = FixedWeightBounds<UnitWeightCost, RuntimeCall, MaxInstructions>;
    type Trader = FreeForAll;
    type ResponseHandler = ();
    type AssetTrap = PolkadotXcm;
    type AssetClaims = PolkadotXcm;
    type SubscriptionService = PolkadotXcm;
    type AssetLocker = PolkadotXcm;
    type AssetExchanger = ();
    type PalletInstancesInfo = ();
    type MaxAssetsIntoHolding = MaxAssetsIntoHolding;
    type FeeManager = ();
    type MessageExporter = ();
    type UniversalAliases = Nothing;
    type CallDispatcher = RuntimeCall;
    type SafeCallFilter = Everything;
    type Aliasers = ();
}

pub struct FreeForAll;

impl WeightTrader for FreeForAll {
    fn new() -> Self {
        Self
    }

    fn buy_weight(
        &mut self,
        weight: Weight,
        payment: Assets,
        _xcm: &XcmContext,
    ) -> Result<Assets, XcmError> {
        log::trace!(target: "fassets::weight", "buy_weight weight: {:?}, payment: {:?}", weight, payment);
        Ok(payment)
    }
}

parameter_types! {
    pub const RelayNetwork: NetworkId = NetworkId::Kusama;
    pub UniversalLocation: InteriorMultiLocation =
        X2(GlobalConsensus(RelayNetwork::get()), Parachain(ParachainInfo::parachain_id().into()));

    pub const UnitWeightCost: Weight = Weight::from_parts(10, 10);
    pub const MaxInstructions: u32 = 100;
    pub const MaxAssetsIntoHolding: u32 = 64;

}

pub type LocalOriginToLocation = SignedToAccountId32<RuntimeOrigin, AccountId, RelayNetwork>;

// Needed decl_test_network
pub type XcmRouter = ParachainXcmRouter<ParachainInfo>;
pub type Barrier = (TakeWeightCredit, AllowTopLevelPaidExecutionFrom<Everything>);

impl pallet_xcm::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type SendXcmOrigin = EnsureXcmOrigin<RuntimeOrigin, LocalOriginToLocation>;
    type XcmRouter = XcmRouter;
    type ExecuteXcmOrigin = EnsureXcmOrigin<RuntimeOrigin, LocalOriginToLocation>;
    type XcmExecuteFilter = Everything;
    type XcmExecutor = XcmExecutor<XcmConfig>;
    type XcmTeleportFilter = Nothing;
    type XcmReserveTransferFilter = Everything;
    type Weigher = FixedWeightBounds<UnitWeightCost, RuntimeCall, MaxInstructions>;
    type UniversalLocation = UniversalLocation;
    type RuntimeOrigin = RuntimeOrigin;
    type RuntimeCall = RuntimeCall;
    const VERSION_DISCOVERY_QUEUE_SIZE: u32 = 100;
    type AdvertisedXcmVersion = pallet_xcm::CurrentXcmVersion;
    type Currency = Balances;
    type CurrencyMatcher = ();
    type TrustedLockers = ();
    type SovereignAccountOf = ();
    type MaxLockers = ConstU32<8>;
    type WeightInfo = pallet_xcm::TestWeightInfo;
    type AdminOrigin = EnsureRoot<AccountId>;
    type MaxRemoteLockConsumers = ConstU32<0>;
    type RemoteLockConsumerIdentifier = ();
    #[cfg(feature = "runtime-benchmarks")]
    type ReachableDest = ReachableDest;
}

type Block = frame_system::mocking::MockBlock<Runtime>;

struct DummyAssetTransactor;
impl TransactAsset for DummyAssetTransactor {
    fn can_check_in(
        _origin: &MultiLocation,
        _what: &MultiAsset,
        _context: &XcmContext,
    ) -> XcmResult {
        Err(XcmError::Unimplemented)
    }

    fn check_in(_origin: &MultiLocation, _what: &MultiAsset, _context: &XcmContext) {}

    fn can_check_out(
        _dest: &MultiLocation,
        _what: &MultiAsset,
        _context: &XcmContext,
    ) -> XcmResult {
        Err(XcmError::Unimplemented)
    }

    fn check_out(_dest: &MultiLocation, _what: &MultiAsset, _context: &XcmContext) {}

    fn deposit_asset(
        _what: &MultiAsset,
        _who: &MultiLocation,
        _context: Option<&XcmContext>,
    ) -> XcmResult {
        Err(XcmError::Unimplemented)
    }

    fn withdraw_asset(
        _what: &MultiAsset,
        _who: &MultiLocation,
        _maybe_context: Option<&XcmContext>,
    ) -> Result<Assets, XcmError> {
        Err(XcmError::Unimplemented)
    }

    fn internal_transfer_asset(
        _asset: &MultiAsset,
        _from: &MultiLocation,
        _to: &MultiLocation,
        _context: &XcmContext,
    ) -> Result<Assets, XcmError> {
        Err(XcmError::Unimplemented)
    }

    fn transfer_asset(
        asset: &MultiAsset,
        from: &MultiLocation,
        to: &MultiLocation,
        context: &XcmContext,
    ) -> Result<Assets, XcmError> {
        match Self::internal_transfer_asset(asset, from, to, context) {
            Err(XcmError::AssetNotFound | XcmError::Unimplemented) => {
                let assets = Self::withdraw_asset(asset, from, Some(context))?;
                // Not a very forgiving attitude; once we implement roll-backs then it'll be nicer.
                Self::deposit_asset(asset, to, Some(context))?;
                Ok(assets)
            }
            result => result,
        }
    }
}

construct_runtime! {
    pub enum Runtime {
        System: frame_system,
        Balances: pallet_balances,

        ParachainInfo: parachain_info,
        ParachainSystem: cumulus_pallet_parachain_system,
        XcmpQueue: cumulus_pallet_xcmp_queue,
        // DmpQueue: cumulus_pallet_dmp_queue,
        // CumulusXcm: cumulus_pallet_xcm,

        // Tokens: orml_tokens,
        // XTokens: orml_xtokens,

        PolkadotXcm: pallet_xcm,
        // OrmlXcm: orml_xcm,
    }

}
