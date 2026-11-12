//! Copies files into a new workspace without duplicating their data when the
//! filesystem can share it (APFS, Btrfs, XFS).

use std::fs;
use std::io;
use std::path::Path;

pub fn clone_path(from: &Path, to: &Path) -> io::Result<()> {
    let meta = fs::symlink_metadata(from)?;
    if meta.is_dir() {
        // APFS clones a whole directory in one call. Other filesystems only
        // clone single files, so the directory is walked.
        if cfg!(target_os = "macos") && reflink_copy::reflink(from, to).is_ok() {
            return Ok(());
        }
        fs::create_dir_all(to)?;
        for entry in fs::read_dir(from)? {
            let entry = entry?;
            clone_path(&entry.path(), &to.join(entry.file_name()))?;
        }
        return Ok(());
    }
    if meta.is_symlink() {
        #[cfg(unix)]
        return std::os::unix::fs::symlink(fs::read_link(from)?, to);
    }
    reflink_copy::reflink_or_copy(from, to)?;
    // A cloned file is created with default permissions on Linux, which
    // would drop the executable bit.
    fs::set_permissions(to, meta.permissions())
}
