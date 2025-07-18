use std::{
    borrow::Borrow,
    ffi::OsStr,
    fmt, ops,
    path::{Path, PathBuf},
};

pub use camino::{Utf8Component, Utf8Components, Utf8Path, Utf8PathBuf, Utf8Prefix};

use paths::{AbsPath as InnerAbsPath, AbsPathBuf as InnerAbsPathBuf, RelPath};

use vfs::VfsPath;

pub trait ToVfsPath {
    fn to_vfs_path(&self) -> VfsPath;
}

impl ToVfsPath for AbsPathBuf {
    fn to_vfs_path(&self) -> VfsPath {
        VfsPath::from(self.0.clone())
    }
}

fn normalize_path(path: &Utf8Path) -> Utf8PathBuf {
    let mut components = path.components().peekable();
    let mut ret = if let Some(c @ Utf8Component::Prefix(..)) = components.peek().copied() {
        components.next();
        Utf8PathBuf::from(c.as_str())
    } else {
        Utf8PathBuf::new()
    };

    for component in components {
        match component {
            Utf8Component::Prefix(..) => unreachable!(),
            Utf8Component::RootDir => {
                ret.push(component.as_str());
            }
            Utf8Component::CurDir => {}
            Utf8Component::ParentDir => {
                ret.pop();
            }
            Utf8Component::Normal(c) => {
                ret.push(c);
            }
        }
    }
    ret
}

#[cfg(target_os = "windows")]
fn normalize_windows(path: &Utf8Path) -> Utf8PathBuf {
    let s = path.as_os_str().to_string_lossy().to_string();
    let stripped = if s.starts_with(r"\\?\") {
        s[4..].to_string()
    } else {
        s
    };
    let replaced = stripped.replace('\\', "/");
    let utf8_path = Utf8Path::new(&replaced);
    normalize_path(utf8_path)
}

#[derive(Debug, Clone, Ord, PartialOrd, Eq, Hash)]
pub struct AbsPathBuf(InnerAbsPathBuf);

impl From<AbsPathBuf> for Utf8PathBuf {
    fn from(AbsPathBuf(path_buf): AbsPathBuf) -> Utf8PathBuf {
        path_buf.into()
    }
}

impl From<AbsPathBuf> for PathBuf {
    fn from(AbsPathBuf(path_buf): AbsPathBuf) -> PathBuf {
        path_buf.into()
    }
}

impl ops::Deref for AbsPathBuf {
    type Target = AbsPath;
    fn deref(&self) -> &AbsPath {
        self.as_path()
    }
}

impl AsRef<Utf8Path> for AbsPathBuf {
    fn as_ref(&self) -> &Utf8Path {
        self.0.as_ref()
    }
}

impl AsRef<OsStr> for AbsPathBuf {
    fn as_ref(&self) -> &OsStr {
        self.0.as_ref()
    }
}

impl AsRef<Path> for AbsPathBuf {
    fn as_ref(&self) -> &Path {
        self.0.as_ref()
    }
}

impl AsRef<AbsPath> for AbsPathBuf {
    fn as_ref(&self) -> &AbsPath {
        self.as_path()
    }
}

impl AsRef<InnerAbsPath> for AbsPathBuf {
    fn as_ref(&self) -> &InnerAbsPath {
        &*self.0
    }
}

impl Borrow<AbsPath> for AbsPathBuf {
    fn borrow(&self) -> &AbsPath {
        self.as_path()
    }
}

impl Borrow<InnerAbsPath> for AbsPathBuf {
    fn borrow(&self) -> &InnerAbsPath {
        &*self.0
    }
}

impl TryFrom<Utf8PathBuf> for AbsPathBuf {
    type Error = Utf8PathBuf;
    fn try_from(path_buf: Utf8PathBuf) -> Result<AbsPathBuf, Utf8PathBuf> {
        if !path_buf.is_absolute() {
            return Err(path_buf);
        }
        #[cfg(target_os = "windows")]
        {
            let normalized = normalize_windows(&path_buf);
            let inner = InnerAbsPathBuf::assert(normalized);
            Ok(AbsPathBuf(inner))
        }
        #[cfg(not(target_os = "windows"))]
        {
            let inner = InnerAbsPathBuf::assert(path_buf);
            Ok(AbsPathBuf(inner))
        }
    }
}

impl TryFrom<&str> for AbsPathBuf {
    type Error = Utf8PathBuf;
    fn try_from(path: &str) -> Result<AbsPathBuf, Utf8PathBuf> {
        AbsPathBuf::try_from(Utf8PathBuf::from(path))
    }
}

impl<P: AsRef<Path> + ?Sized> PartialEq<P> for AbsPathBuf {
    fn eq(&self, other: &P) -> bool {
        #[cfg(target_os = "windows")]
        {
            let self_str = self.as_str().to_lowercase();
            let other_str = other.as_ref().to_string_lossy().to_lowercase();
            self_str == other_str
        }
        #[cfg(not(target_os = "windows"))]
        {
            self.0 == other.as_ref()
        }
    }
}

impl AbsPathBuf {
    pub fn assert(path: Utf8PathBuf) -> AbsPathBuf {
        AbsPathBuf::try_from(path)
            .unwrap_or_else(|path| panic!("expected absolute path, got {path}"))
    }

    pub fn assert_utf8(path: PathBuf) -> AbsPathBuf {
        AbsPathBuf::assert(
            Utf8PathBuf::from_path_buf(path)
                .unwrap_or_else(|path| panic!("expected utf8 path, got {}", path.display())),
        )
    }

    pub fn assert_inner(path: &InnerAbsPathBuf) -> &AbsPathBuf {
        unsafe { &*(path as *const InnerAbsPathBuf as *const AbsPathBuf) }
    }

    pub fn as_path(&self) -> &AbsPath {
        AbsPath::assert_inner(self.0.as_path())
    }

    pub fn inner(&self) -> &InnerAbsPathBuf {
        &self.0
    }

    pub fn pop(&mut self) -> bool {
        self.0.pop()
    }

    pub fn push<P: AsRef<Utf8Path>>(&mut self, suffix: P) {
        self.0.push(suffix);
        #[cfg(target_os = "windows")]
        {
            let normalized = normalize_windows(self.as_ref());
            self.0 = InnerAbsPathBuf::assert(normalized);
        }
    }

    pub fn join(&self, path: impl AsRef<Utf8Path>) -> Self {
        #[cfg(target_os = "windows")]
        {
            let joined = Utf8Path::join(self.as_ref(), path.as_ref());
            let normalized = normalize_windows(&joined);
            AbsPathBuf(InnerAbsPathBuf::assert(normalized))
        }
        #[cfg(not(target_os = "windows"))]
        {
            AbsPathBuf(self.0.join(path.as_ref()))
        }
    }
}

impl fmt::Display for AbsPathBuf {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.0, f)
    }
}

#[derive(Debug, Ord, PartialOrd, Eq, Hash)]
#[repr(transparent)]
pub struct AbsPath(InnerAbsPath);

impl<P: AsRef<Path> + ?Sized> PartialEq<P> for AbsPath {
    fn eq(&self, other: &P) -> bool {
        #[cfg(target_os = "windows")]
        {
            let self_str = self.as_str().to_lowercase();
            let other_str = other.as_ref().to_string_lossy().to_lowercase();
            self_str == other_str
        }
        #[cfg(not(target_os = "windows"))]
        {
            self.0 == other.as_ref()
        }
    }
}

impl ops::Deref for AbsPath {
    type Target = InnerAbsPath;
    fn deref(&self) -> &InnerAbsPath {
        &self.0
    }
}

impl AsRef<Utf8Path> for AbsPath {
    fn as_ref(&self) -> &Utf8Path {
        self.0.as_ref()
    }
}

impl AsRef<Path> for AbsPath {
    fn as_ref(&self) -> &Path {
        self.0.as_ref()
    }
}

impl AsRef<OsStr> for AbsPath {
    fn as_ref(&self) -> &OsStr {
        self.0.as_ref()
    }
}

impl AsRef<InnerAbsPath> for AbsPath {
    fn as_ref(&self) -> &InnerAbsPath {
        &self.0
    }
}

impl ToOwned for AbsPath {
    type Owned = AbsPathBuf;

    fn to_owned(&self) -> Self::Owned {
        AbsPathBuf(self.0.to_owned())
    }
}

impl<'a> TryFrom<&'a Utf8Path> for &'a AbsPath {
    type Error = &'a Utf8Path;
    fn try_from(path: &'a Utf8Path) -> Result<&'a AbsPath, &'a Utf8Path> {
        if !path.is_absolute() {
            return Err(path);
        }
        Ok(AbsPath::assert(path))
    }
}

impl AbsPath {
    pub fn assert(path: &Utf8Path) -> &AbsPath {
        let inner = InnerAbsPath::assert(path);
        unsafe { &*(inner as *const InnerAbsPath as *const AbsPath) }
    }

    pub fn assert_inner(path: &InnerAbsPath) -> &AbsPath {
        unsafe { &*(path as *const InnerAbsPath as *const AbsPath) }
    }

    pub fn parent(&self) -> Option<&AbsPath> {
        self.0.parent().map(AbsPath::assert_inner)
    }

    pub fn absolutize(&self, path: impl AsRef<Utf8Path>) -> AbsPathBuf {
        self.join(path).normalize()
    }

    pub fn join(&self, path: impl AsRef<Utf8Path>) -> AbsPathBuf {
        #[cfg(target_os = "windows")]
        {
            let joined = Utf8Path::join(self.as_ref(), path.as_ref());
            let normalized = normalize_windows(&joined);
            AbsPathBuf(InnerAbsPathBuf::assert(normalized))
        }
        #[cfg(not(target_os = "windows"))]
        {
            AbsPathBuf(self.0.join(path.as_ref()))
        }
    }

    pub fn normalize(&self) -> AbsPathBuf {
        #[cfg(target_os = "windows")]
        {
            let normalized = normalize_windows(self.as_ref());
            AbsPathBuf(InnerAbsPathBuf::assert(normalized))
        }
        #[cfg(not(target_os = "windows"))]
        {
            AbsPathBuf(self.0.normalize())
        }
    }

    pub fn to_path_buf(&self) -> AbsPathBuf {
        AbsPathBuf(self.0.to_path_buf())
    }

    pub fn canonicalize(&self) -> ! {
        panic!(
            "We explicitly do not provide canonicalization API, as that is almost always a wrong solution, see #14430"
        )
    }

    pub fn strip_prefix(&self, base: &AbsPath) -> Option<&RelPath> {
        self.0.strip_prefix(&base.0)
    }

    pub fn starts_with(&self, base: &AbsPath) -> bool {
        self.0.starts_with(&base.0)
    }

    pub fn ends_with(&self, suffix: &RelPath) -> bool {
        self.0.ends_with(suffix)
    }

    pub fn name_and_extension(&self) -> Option<(&str, Option<&str>)> {
        Some((self.file_stem()?, self.extension()))
    }

    pub fn file_name(&self) -> Option<&str> {
        self.0.file_name()
    }

    pub fn extension(&self) -> Option<&str> {
        self.0.extension()
    }

    pub fn file_stem(&self) -> Option<&str> {
        self.0.file_stem()
    }

    pub fn as_os_str(&self) -> &OsStr {
        self.0.as_os_str()
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }

    #[deprecated(note = "use Display instead")]
    pub fn display(&self) -> ! {
        unimplemented!()
    }

    #[deprecated(note = "use std::fs::metadata().is_ok() instead")]
    pub fn exists(&self) -> ! {
        unimplemented!()
    }

    pub fn components(&self) -> Utf8Components<'_> {
        self.0.components()
    }
}

impl fmt::Display for AbsPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.0, f)
    }
}