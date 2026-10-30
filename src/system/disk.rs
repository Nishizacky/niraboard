use std::ffi::CString;
use std::fs;
use std::path::Path;

const MOUNT_POINT: &str = "/";

#[derive(Debug, Clone, PartialEq)]
pub struct Disk {
    pub mount: String,
    pub filesystem: String,
    pub total_bytes: u64,
    pub used_bytes: u64,
    pub available_bytes: u64,
    pub percent: u8,
}

/// Read usage for the root filesystem.
///
/// `statvfs` reports block counts, so this is cheap enough to run on every
/// tick. The filesystem type comes from the mount table and is shown as
/// `unknown` when that lookup fails; the usage figures remain useful either way.
pub fn read() -> Option<Disk> {
    let (total_bytes, used_bytes, available_bytes) = usage(Path::new(MOUNT_POINT))?;

    Some(Disk {
        mount: MOUNT_POINT.to_owned(),
        filesystem: filesystem_type(MOUNT_POINT).unwrap_or_else(|| "unknown".to_owned()),
        total_bytes,
        used_bytes,
        available_bytes,
        percent: percentage(used_bytes, total_bytes),
    })
}

/// Return `(total, used, available)` in bytes for the filesystem at `path`.
fn usage(path: &Path) -> Option<(u64, u64, u64)> {
    let path = CString::new(path.as_os_str().as_encoded_bytes()).ok()?;
    let mut stat: libc::statvfs = unsafe { std::mem::zeroed() };

    // SAFETY: `stat` is a valid, writable statvfs structure and `path` is a
    // NUL-terminated C string that outlives the call.
    if unsafe { libc::statvfs(path.as_ptr(), &mut stat) } != 0 {
        return None;
    }

    let block_size = if stat.f_frsize > 0 {
        stat.f_frsize
    } else {
        stat.f_bsize
    } as u64;
    if block_size == 0 {
        return None;
    }

    let total = (stat.f_blocks as u64).saturating_mul(block_size);
    let free = (stat.f_bfree as u64).saturating_mul(block_size);
    let available = (stat.f_bavail as u64).saturating_mul(block_size);

    Some((
        total,
        total.saturating_sub(free),
        available.min(total),
    ))
}

/// Filesystem type for `mount_point`, parsed from the kernel mount table.
fn filesystem_type(mount_point: &str) -> Option<String> {
    let mounts = fs::read_to_string("/proc/self/mounts")
        .or_else(|_| fs::read_to_string("/proc/mounts"))
        .ok()?;

    parse_filesystem_type(&mounts, mount_point)
}

fn parse_filesystem_type(text: &str, mount_point: &str) -> Option<String> {
    for line in text.lines() {
        let mut fields = line.split_whitespace();
        let (Some(_device), Some(mount), Some(filesystem)) =
            (fields.next(), fields.next(), fields.next())
        else {
            continue;
        };

        if mount == mount_point {
            return Some(filesystem.to_owned());
        }
    }

    None
}

fn percentage(used: u64, total: u64) -> u8 {
    if total == 0 {
        return 0;
    }

    ((used as f64 / total as f64) * 100.0)
        .round()
        .clamp(0.0, 100.0) as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    const MOUNTS: &str = "\
proc /proc proc rw,nosuid,nodev,noexec,relatime 0 0
/dev/nvme0n1p2 / ext4 rw,relatime 0 0
/dev/nvme0n1p1 /boot vfat rw,relatime,fmask=0077 0 0
tmpfs /run tmpfs rw,nosuid,nodev 0 0
";

    #[test]
    fn finds_the_root_filesystem_type() {
        assert_eq!(parse_filesystem_type(MOUNTS, "/").as_deref(), Some("ext4"));
        assert_eq!(
            parse_filesystem_type(MOUNTS, "/boot").as_deref(),
            Some("vfat")
        );
    }

    #[test]
    fn returns_none_for_an_unknown_mount_point() {
        assert_eq!(parse_filesystem_type(MOUNTS, "/home"), None);
    }

    #[test]
    fn computes_whole_percentages() {
        assert_eq!(percentage(0, 100), 0);
        assert_eq!(percentage(42, 100), 42);
        assert_eq!(percentage(200, 100), 100);
        assert_eq!(percentage(0, 0), 0);
    }
}
