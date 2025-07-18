use std::{
    borrow::Borrow,
    ffi::OsStr,
    fmt, ops,
    path::{Path, PathBuf},
};

pub use camino::{Utf8Component, Utf8Components, Utf8Path, Utf8PathBuf, Utf8Prefix};

use paths::{AbsPath as InnerAbsPath, AbsPathBuf as InnerAbsPathBuf, RelPath};

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
pub struct WindowsAbsPathBuf(InnerAbsPathBuf);

impl ops::Deref for WindowsAbsPathBuf {
    type Target = InnerAbsPathBuf;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl ops::DerefMut for WindowsAbsPathBuf {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl From<WindowsAbsPathBuf> for Utf8PathBuf {
    fn from(w: WindowsAbsPathBuf) -> Utf8PathBuf {
        w.0.into()
    }
}

impl From<WindowsAbsPathBuf> for PathBuf {
    fn from(w: WindowsAbsPathBuf) -> PathBuf {
        w.0.into()
    }
}

impl AsRef<Utf8Path> for WindowsAbsPathBuf {
    fn as_ref(&self) -> &Utf8Path {
        self.0.as_ref()
    }
}

impl AsRef<OsStr> for WindowsAbsPathBuf {
    fn as_ref(&self) -> &OsStr {
        self.0.as_ref()
    }
}

impl AsRef<Path> for WindowsAbsPathBuf {
    fn as_ref(&self) -> &Path {
        self.0.as_ref()
    }
}

impl AsRef<WindowsAbsPath> for WindowsAbsPathBuf {
    fn as_ref(&self) -> &WindowsAbsPath {
        self.as_path()
    }
}

impl Borrow<WindowsAbsPath> for WindowsAbsPathBuf {
    fn borrow(&self) -> &WindowsAbsPath {
        self.as_path()
    }
}

impl TryFrom<Utf8PathBuf> for WindowsAbsPathBuf {
    type Error = Utf8PathBuf;
    fn try_from(path_buf: Utf8PathBuf) -> Result<WindowsAbsPathBuf, Utf8PathBuf> {
        if !path_buf.is_absolute() {
            return Err(path_buf);
        }
        #[cfg(target_os = "windows")]
        {
            let normalized = normalize_windows(&path_buf);
            Ok(WindowsAbsPathBuf(InnerAbsPathBuf::try_from(normalized)?))
        }
        #[cfg(not(target_os = "windows"))]
        Ok(WindowsAbsPathBuf(InnerAbsPathBuf::try_from(path_buf)?))
    }
}

impl TryFrom<&str> for WindowsAbsPathBuf {
    type Error = Utf8PathBuf;
    fn try_from(path: &str) -> Result<WindowsAbsPathBuf, Utf8PathBuf> {
        WindowsAbsPathBuf::try_from(Utf8PathBuf::from(path))
    }
}

impl<P: AsRef<Path> + ?Sized> PartialEq<P> for WindowsAbsPathBuf {
    fn eq(&self, other: &P) -> bool {
        #[cfg(target_os = "windows")]
        {
            let self_str = self.0.as_str().to_lowercase();
            let other_str = other.as_ref().to_string_lossy().to_lowercase();
            self_str == other_str
        }
        #[cfg(not(target_os = "windows"))]
        {
            self.0.as_std_path() == other.as_ref()
        }
    }
}

impl WindowsAbsPathBuf {
    pub fn assert(path: Utf8PathBuf) -> WindowsAbsPathBuf {
        WindowsAbsPathBuf::try_from(path)
            .unwrap_or_else(|path| panic!("expected absolute path, got {path}"))
    }

    pub fn assert_utf8(path: PathBuf) -> WindowsAbsPathBuf {
        WindowsAbsPathBuf::assert(
            Utf8PathBuf::from_path_buf(path)
                .unwrap_or_else(|path| panic!("expected utf8 path, got {}", path.display())),
        )
    }

    pub fn as_path(&self) -> &WindowsAbsPath {
        WindowsAbsPath::assert(self.0.as_path().as_ref())
    }

    pub fn push<P: AsRef<Utf8Path>>(&mut self, suffix: P) {
        self.0.push(suffix.as_ref());
        #[cfg(target_os = "windows")]
        {
            self.0 = InnerAbsPathBuf::try_from(normalize_windows(self.0.as_ref())).unwrap();
        }
    }

    pub fn join(&self, path: impl AsRef<Utf8Path>) -> Self {
        let joined_inner = InnerAbsPathBuf::try_from(Utf8Path::join(self.as_ref(), path.as_ref()).to_path_buf()).unwrap();
        let mut result = Self(joined_inner);
        #[cfg(target_os = "windows")]
        {
            result.0 = InnerAbsPathBuf::try_from(normalize_windows(result.as_ref())).unwrap();
        }
        result
    }
}

impl fmt::Display for WindowsAbsPathBuf {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.0, f)
    }
}

#[derive(Debug, Ord, PartialOrd, Eq, Hash)]
#[repr(transparent)]
pub struct WindowsAbsPath(InnerAbsPath);

impl ops::Deref for WindowsAbsPath {
    type Target = InnerAbsPath;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<P: AsRef<Path> + ?Sized> PartialEq<P> for WindowsAbsPath {
    fn eq(&self, other: &P) -> bool {
        #[cfg(target_os = "windows")]
        {
            let self_str = self.0.as_str().to_lowercase();
            let other_str = other.as_ref().to_string_lossy().to_lowercase();
            self_str == other_str
        }
        #[cfg(not(target_os = "windows"))]
        {
            self.0.as_std_path() == other.as_ref()
        }
    }
}

impl AsRef<Utf8Path> for WindowsAbsPath {
    fn as_ref(&self) -> &Utf8Path {
        self.0.as_ref()
    }
}

impl AsRef<Path> for WindowsAbsPath {
    fn as_ref(&self) -> &Path {
        self.0.as_ref()
    }
}

impl AsRef<OsStr> for WindowsAbsPath {
    fn as_ref(&self) -> &OsStr {
        self.0.as_ref()
    }
}

impl ToOwned for WindowsAbsPath {
    type Owned = WindowsAbsPathBuf;

    fn to_owned(&self) -> Self::Owned {
        WindowsAbsPathBuf(self.0.to_owned())
    }
}

impl<'a> TryFrom<&'a Utf8Path> for &'a WindowsAbsPath {
    type Error = &'a Utf8Path;
    fn try_from(path: &'a Utf8Path) -> Result<&'a WindowsAbsPath, &'a Utf8Path> {
        if !path.is_absolute() {
            return Err(path);
        }
        Ok(WindowsAbsPath::assert(path))
    }
}

impl WindowsAbsPath {
    pub fn assert(path: &Utf8Path) -> &WindowsAbsPath {
        assert!(path.is_absolute(), "{path} is not absolute");
        unsafe { &*(InnerAbsPath::assert(path) as *const InnerAbsPath as *const WindowsAbsPath) }
    }

    pub fn parent(&self) -> Option<&WindowsAbsPath> {
        self.0.parent().map(|p| WindowsAbsPath::assert(p.as_ref()))
    }

    pub fn absolutize(&self, path: impl AsRef<Utf8Path>) -> WindowsAbsPathBuf {
        let joined = self.join(path);
        #[cfg(target_os = "windows")]
        return WindowsAbsPathBuf(InnerAbsPathBuf::try_from(normalize_windows(joined.as_ref())).unwrap());
        #[cfg(not(target_os = "windows"))]
        joined
    }

    pub fn join(&self, path: impl AsRef<Utf8Path>) -> WindowsAbsPathBuf {
        WindowsAbsPathBuf(InnerAbsPathBuf::try_from(Utf8Path::join(self.as_ref(), path.as_ref()).to_path_buf()).unwrap())
    }

    pub fn normalize(&self) -> WindowsAbsPathBuf {
        #[cfg(target_os = "windows")]
        {
            WindowsAbsPathBuf(InnerAbsPathBuf::try_from(normalize_windows(self.as_ref())).unwrap())
        }
        #[cfg(not(target_os = "windows"))]
        {
            WindowsAbsPathBuf(InnerAbsPathBuf(normalize_path(self.as_ref())))
        }
    }

    pub fn to_path_buf(&self) -> WindowsAbsPathBuf {
        WindowsAbsPathBuf(self.0.to_path_buf())
    }

    pub fn strip_prefix(&self, base: &WindowsAbsPath) -> Option<&RelPath> {
        <Self as AsRef<Utf8Path>>::as_ref(self).strip_prefix(<_ as AsRef<Utf8Path>>::as_ref(base)).ok().map(RelPath::new_unchecked)
    }

    pub fn starts_with(&self, base: &WindowsAbsPath) -> bool {
        <Self as AsRef<Utf8Path>>::as_ref(self).starts_with(<_ as AsRef<Utf8Path>>::as_ref(base))
    }

    pub fn ends_with(&self, suffix: &RelPath) -> bool {
        <Self as AsRef<Utf8Path>>::as_ref(self).ends_with(suffix.as_utf8_path())
    }

    pub fn name_and_extension(&self) -> Option<(&str, Option<&str>)> {
        Some((self.file_stem()?, self.extension()))
    }
}

impl fmt::Display for WindowsAbsPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.0, f)
    }
}