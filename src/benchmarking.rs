use super::*;

use frame_benchmarking::v2::*;
use frame_system::RawOrigin;

#[benchmarks]
mod benchmarks {
    use super::*;

    #[benchmark]
    fn register_asset() {
        let big_asset_id = VersionedAssetId::V3(AssetId::Concrete(MultiLocation {
            parents: 0,
            interior: X8(
                GeneralKey {
                    length: 32,
                    data: [0xff; 32],
                },
                GeneralKey {
                    length: 32,
                    data: [0xff; 32],
                },
                GeneralKey {
                    length: 32,
                    data: [0xff; 32],
                },
                GeneralKey {
                    length: 32,
                    data: [0xff; 32],
                },
                GeneralKey {
                    length: 32,
                    data: [0xff; 32],
                },
                GeneralKey {
                    length: 32,
                    data: [0xff; 32],
                },
                GeneralKey {
                    length: 32,
                    data: [0xff; 32],
                },
                GeneralKey {
                    length: 32,
                    data: [0xff; 32],
                },
            ),
        }));

        #[extrinsic_call]
        _(RawOrigin::Root, Box::new(big_asset_id))
    }
}
