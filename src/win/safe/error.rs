use std::error::Error;
use std::fmt;

use crate::win::uses::*;

#[derive(Debug, Clone)]
pub struct WinError {
    error_code: DWORD,
    comment: Option<String>,
    message: Option<String>,
    source: Option<Box<WinError>>,
}

impl WinError {
    fn _new(error_code: DWORD, comment: Option<String>, message: Option<String>, source: Option<Box<WinError>>) -> Self {
        WinError {
            error_code,
            comment,
            message,
            source,
        }
    }

    pub fn new(error_code: DWORD) -> Self {
        Self::_new(error_code, None, None, None)
    }

    pub fn new_with_message(error_code: DWORD) -> Self {
        Self::_new(error_code, None, None, None).with_message()
    }

    /// Call `GetLastError()` but do not attempt to get formatted message from system.
    pub fn get() -> Self {
        let error_code = Self::get_last_error();
        Self::new(error_code)
    }

    pub fn with_comment<S: Into<String>>(&self, comment: S) -> Self {
        let mut clone = self.clone();
        clone.comment = Some(comment.into());
        clone
    }

    /// If formatted message is not initialized, get one via `FormatMessage(...)` and return new error instance.
    pub fn with_message(&self) -> Self {
        match self.message.as_ref() {
            Some(_) => self.clone(),
            None => self.clone_with_message()
        }
    }

    pub fn with_source(&self, source: Self) -> Self {
        let mut clone = self.clone();
        clone.source = Some(Box::new(source));
        clone
    }

    /// Call `GetLastError()` & `FormatMessage(...)` at once.
    pub fn get_with_message() -> Self {
        Self::get().with_message()
    }

    /// Call `FormatMessage(...)` for given error code.
    /// If `FormatMessage(...)` fails, create new error for its status code wrapping original one as a source.
    fn clone_with_message(&self) -> Self {
        match Self::format_message_from_error_code(self.error_code) {
            Ok(message) => {
                let mut clone = self.clone();
                clone.message = Some(message);
                clone
            }
            Err(format_error) => {
                let message = Self::get_format_message_error(self.error_code);
                Self::_new(format_error, None, Some(message), Some(Box::new(self.clone())))
            }
        }
    }

    // getter
    fn error_code(&self) -> DWORD {
        self.error_code
    }

    #[inline(always)]
    fn get_last_error() -> DWORD {
        unsafe { GetLastError() }
    }

    fn format_message_from_error_code(error_code: DWORD) -> Result<String, DWORD> {
        unsafe {
            let mut buffer: LPWSTR = null_mut();
            // If the function succeeds, the return value is the number of TCHARs stored in the output buffer, excluding the terminating null character.
            let len = FormatMessageW(
                FORMAT_MESSAGE_IGNORE_INSERTS
                    | FORMAT_MESSAGE_FROM_SYSTEM
                    | FORMAT_MESSAGE_ALLOCATE_BUFFER, // dwFlags
                null(), // lpSource
                error_code, // dwMessageId
                MAKELANGID(LANG_NEUTRAL, SUBLANG_DEFAULT) as DWORD, // dwLanguageId
                ::std::mem::transmute(&mut buffer as *mut LPWSTR),  // lpBuffer
                0, // nSize
                null_mut(), // va_args
            );

            // If the function fails, the return value is zero. To get extended error information, call GetLastError.
            if len == 0 {
                return Err(Self::get_last_error());
            }

            let message_u16 = U16Str::from_ptr(buffer, len as usize);
            let message_string = message_u16.to_string_lossy();

            if buffer != null_mut() {
                LocalFree(buffer as HLOCAL);
            }

            Ok(message_string)
        }
    }

    fn get_format_message_error(original_error_code: DWORD) -> String {
        format!("FormatMessageW failed while formatting error 0x{:08X}", original_error_code)
    }

    const UNKNOWN_ERROR: &'static str = "UNKNOWN ERROR CODE";
}


unsafe impl Sync for WinError {}

unsafe impl Send for WinError {}

impl fmt::Display for WinError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(comment) = self.comment.as_ref() {
            write!(f, "{}; ", comment)?;
        }
        let message = match self.message.as_ref() {
            Some(msg) => msg.as_str(),
            None => Self::UNKNOWN_ERROR,
        };
        write!(f, "Error Code 0x{:08X}: {}", self.error_code, message)?;

        if let Some(source) = self.source.as_ref() {
            write!(f, "; Caused by: {}", source)?;
        }
        Ok(())
    }
}

impl Error for WinError {
    fn description(&self) -> &str {
        unimplemented!()
    }
}

/// Rust + Windows extension for error handling
pub type WinResult<T> = Result<T, WinError>;
