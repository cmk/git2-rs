use std::{ffi::CStr, marker::PhantomData, path::Path, ptr};

use crate::util::{self, Binding};
use crate::{raw, AttrCheckFlags, Error, Repository};

/// Explicit system attribute source for an isolated read session.
#[derive(Clone, Copy, Debug)]
pub enum AttributeSource<'path> {
    /// Disable system source discovery and macro preloading.
    Absent,
    /// Read and retain this required absolute file, including its macros.
    File(&'path Path),
}

/// An attribute value independent of the native cache lifetime.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum OwnedAttrValue {
    /// Set to true.
    True,
    /// Set to false.
    False,
    /// A byte string, not necessarily UTF-8.
    Bytes(Vec<u8>),
    /// Unspecified, including an explicit reset.
    Unspecified,
}

/// Exclusive attribute queries with an explicit system source and path kind.
///
/// The repository must have an unused attribute/ignore cache and no outstanding
/// index handles. Previously dropped index handles do not prevent acquisition.
/// Source-order flags are fixed for this session. Other source discovery remains native;
/// callers building private views must configure those sources separately.
/// Drop clears the session-owned repository cache. Returned values are owned.
///
/// Construction rejects outstanding index handles, whose filter operations can
/// access the repository without a Rust lifetime. The exclusive repository
/// borrow prevents acquiring further handles or directly querying its cache:
///
/// ```compile_fail
/// use git2::{AttrCheckFlags, AttributeReadSession, AttributeSource, Repository};
/// fn conflicting(repo: &mut Repository) -> Result<(), git2::Error> {
///     let mut session = AttributeReadSession::new(
///         repo, AttributeSource::Absent, AttrCheckFlags::default())?;
///     repo.get_attr(std::path::Path::new("file"), "filter", AttrCheckFlags::default())?;
///     session.get(std::path::Path::new("file"), false, "filter")?;
///     Ok(())
/// }
/// ```
pub struct AttributeReadSession<'repo> {
    raw: *mut raw::git_attr_read_session,
    _repo: PhantomData<&'repo mut Repository>,
}

impl<'repo> AttributeReadSession<'repo> {
    /// Acquire an explicit system source before any native macro preloading.
    ///
    /// A supplied file must be absolute, readable and regular. Its pathname
    /// is copied and its parsed contents retained, so later deletion is inert.
    /// System rules remain repository-wide even if the source file is also a
    /// nested worktree `.gitattributes`; the two roles are cached separately.
    /// `NO_SYSTEM` suppresses its matching rules, not its macro preload, just
    /// as in the native parser. This API requires the bundled library.
    pub fn new(
        repo: &'repo mut Repository,
        source: AttributeSource<'_>,
        flags: AttrCheckFlags,
    ) -> Result<Self, Error> {
        if !raw::vendored() {
            return Err(Error::from_str(
                "attribute read sessions require bundled libgit2",
            ));
        }
        let path = match source {
            AttributeSource::Absent => None,
            AttributeSource::File(path) => {
                if !path.is_absolute() {
                    return Err(Error::from_str("system attribute path must be absolute"));
                }
                Some(util::cstring_to_repo_path(path)?)
            }
        };
        let mut out = ptr::null_mut();
        unsafe {
            try_call!(raw::git_attr_read_session_new(
                &mut out,
                repo.raw(),
                path.as_ref().map_or(ptr::null(), |path| path.as_ptr()),
                flags.bits()
            ));
        }
        Ok(Self {
            raw: out,
            _repo: PhantomData,
        })
    }

    /// Query a repository-relative path using its captured directory kind.
    ///
    /// The operand need not exist in the private worktree. Attribute sources
    /// still use native lookup; this method does not capture other source files.
    pub fn get(&mut self, path: &Path, is_dir: bool, name: &str) -> Result<OwnedAttrValue, Error> {
        let path = util::cstring_to_repo_path(path)?;
        let name = std::ffi::CString::new(name)?;
        let mut value = ptr::null();
        unsafe {
            try_call!(raw::git_attr_read_session_get(
                &mut value,
                self.raw,
                path,
                is_dir as libc::c_int,
                name
            ));
            Ok(match raw::git_attr_value(value) {
                raw::GIT_ATTR_VALUE_TRUE => OwnedAttrValue::True,
                raw::GIT_ATTR_VALUE_FALSE => OwnedAttrValue::False,
                raw::GIT_ATTR_VALUE_STRING => {
                    OwnedAttrValue::Bytes(CStr::from_ptr(value).to_bytes().to_vec())
                }
                raw::GIT_ATTR_VALUE_UNSPECIFIED => OwnedAttrValue::Unspecified,
                _ => return Err(Error::from_str("unknown native attribute value kind")),
            })
        }
    }
}

impl Drop for AttributeReadSession<'_> {
    fn drop(&mut self) {
        unsafe { raw::git_attr_read_session_free(self.raw) }
    }
}
