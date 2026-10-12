use anyhow::Result;

use crate::android::susfs::{
    api::{
        magic::{
            CMD_SUSFS_SHOW_ENABLED_FEATURES, CMD_SUSFS_SHOW_VARIANT, CMD_SUSFS_SHOW_VERSION,
            ERR_CMD_NOT_SUPPORTED, SUSFS_ENABLED_FEATURES_SIZE, SUSFS_MAX_VARIANT_BUFSIZE,
            SUSFS_MAX_VERSION_BUFSIZE,
        },
        susfsctl::{parse_err, susfsctl},
    },
    utils::c_array_to_string,
};

#[repr(C)]
struct SusfsEnabledFeatures {
    enabled_features: [u8; SUSFS_ENABLED_FEATURES_SIZE],
    err: i32,
}

#[repr(C)]
struct SusfsVariant {
    susfs_variant: [u8; SUSFS_MAX_VARIANT_BUFSIZE],
    err: i32,
}

#[repr(C)]
struct SusfsVersion {
    susfs_version: [u8; SUSFS_MAX_VERSION_BUFSIZE],
    err: i32,
}

pub fn version() -> Result<String> {
    let mut info = SusfsVersion {
        susfs_version: [0; SUSFS_MAX_VERSION_BUFSIZE],
        err: ERR_CMD_NOT_SUPPORTED,
    };
    susfsctl(CMD_SUSFS_SHOW_VERSION, &mut info);
    parse_err(CMD_SUSFS_SHOW_VERSION, info.err)?;

    let ver = c_array_to_string(&info.susfs_version);

    if ver.starts_with('v') {
        Ok(ver)
    } else {
        Ok("Unsupported".to_string())
    }
}

pub fn probe() -> Result<Option<String>> {
    let mut info = SusfsVersion {
        susfs_version: [0; SUSFS_MAX_VERSION_BUFSIZE],
        err: ERR_CMD_NOT_SUPPORTED,
    };
    let result = susfsctl(CMD_SUSFS_SHOW_VERSION, &mut info);
    if info.err == ERR_CMD_NOT_SUPPORTED && info.susfs_version.iter().all(|byte| *byte == 0) {
        let error = std::io::Error::last_os_error();
        if result == -1
            && matches!(
                error.raw_os_error(),
                Some(libc::EINVAL) | Some(libc::ENOSYS)
            )
        {
            let mut variant = SusfsVariant {
                susfs_variant: [0; SUSFS_MAX_VARIANT_BUFSIZE],
                err: ERR_CMD_NOT_SUPPORTED,
            };
            let result = susfsctl(CMD_SUSFS_SHOW_VARIANT, &mut variant);
            let error = std::io::Error::last_os_error();
            if variant.err == ERR_CMD_NOT_SUPPORTED
                && variant.susfs_variant.iter().all(|byte| *byte == 0)
                && result == -1
                && matches!(
                    error.raw_os_error(),
                    Some(libc::EINVAL) | Some(libc::ENOSYS)
                )
            {
                return Ok(None);
            }
            anyhow::bail!(
                "A SUSFS variant responded without a recognized version; loading was refused"
            );
        }
        anyhow::bail!("Cannot establish that SUSFS is absent: result={result}, error={error}");
    }
    parse_err(CMD_SUSFS_SHOW_VERSION, info.err)?;
    let version = c_array_to_string(&info.susfs_version);
    anyhow::ensure!(
        version.starts_with('v'),
        "Unrecognized SUSFS version response"
    );
    Ok(Some(version))
}

pub fn variant() -> Result<String> {
    let mut info = SusfsVariant {
        susfs_variant: [0; SUSFS_MAX_VARIANT_BUFSIZE],
        err: ERR_CMD_NOT_SUPPORTED,
    };
    susfsctl(CMD_SUSFS_SHOW_VARIANT, &mut info);
    parse_err(CMD_SUSFS_SHOW_VARIANT, info.err)?;

    let variant = c_array_to_string(&info.susfs_variant);
    Ok(variant)
}

pub fn enabled_features() -> Result<String> {
    let mut info = Box::new(SusfsEnabledFeatures {
        enabled_features: [0; SUSFS_ENABLED_FEATURES_SIZE],
        err: ERR_CMD_NOT_SUPPORTED,
    });
    susfsctl(CMD_SUSFS_SHOW_ENABLED_FEATURES, &mut *info);
    parse_err(CMD_SUSFS_SHOW_ENABLED_FEATURES, info.err)?;

    let features = c_array_to_string(&info.enabled_features);

    Ok(features)
}
