#![allow(missing_docs)]

use super::*;

use frame_benchmarking::v2::*;

#[benchmarks]
pub mod benchmarks {
    use super::*;

    #[benchmark]
    pub fn foreign_asset_registration_checks() -> Result<(), BenchmarkError> {
        let asset_id = AssetId::Concrete(MultiLocation {
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
        });
        let versioned_asset_id = VersionedAssetId::V3(asset_id);

        let origin = T::RegisterOrigin::try_successful_origin(&asset_id).unwrap();

        #[block]
        {
            <Pallet<T>>::foreign_asset_registration_checks(origin, Box::new(versioned_asset_id))?;
        }

        Ok(())
    }
}
