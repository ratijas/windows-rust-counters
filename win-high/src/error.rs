use std::error::Error;
use std::fmt;

use crate::prelude::v2::*;

use win_low::um::winnt::*;

#[derive(Debug, Clone)]
pub struct WinError {
    error_code: WIN32_ERROR,
    comment: Option<String>,
    message: Option<String>,
    source: Option<Box<WinError>>,
}

impl WinError {
    fn _new(error_code: WIN32_ERROR, comment: Option<String>, message: Option<String>, source: Option<Box<WinError>>) -> Self {
        WinError {
            error_code,
            comment,
            message,
            source,
        }
    }

    pub fn new(error_code: WIN32_ERROR) -> Self {
        Self::_new(error_code, None, None, None)
    }

    pub fn new_with_message(error_code: WIN32_ERROR) -> Self {
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

    /// Error code getter.
    pub fn error_code(&self) -> WIN32_ERROR {
        self.error_code
    }

    #[inline(always)]
    fn get_last_error() -> WIN32_ERROR {
        unsafe { GetLastError() }
    }

    fn format_message_from_error_code(error_code: WIN32_ERROR) -> Result<String, WIN32_ERROR> {
        unsafe {
            let mut buffer: PWSTR = PWSTR::null();
            // If the function succeeds, the return value is the number of TCHARs stored in the output buffer, excluding the terminating null character.
            let len = FormatMessageW(
                FORMAT_MESSAGE_IGNORE_INSERTS
                    | FORMAT_MESSAGE_FROM_SYSTEM
                    | FORMAT_MESSAGE_ALLOCATE_BUFFER, // dwFlags
                None, // lpSource
                error_code.0, // dwMessageId
                MAKELANGID(LANG_NEUTRAL as _, SUBLANG_DEFAULT as _) as _, // dwLanguageId
                ::std::mem::transmute(&mut buffer as *mut PWSTR),  // lpBuffer
                0, // nSize
                None, // va_args
            );

            // If the function fails, the return value is zero. To get extended error information, call GetLastError.
            if len == 0 {
                return Err(Self::get_last_error());
            }

            let message_u16 = U16Str::from_ptr(buffer.0, len as usize);
            let message_string = message_u16.to_string_lossy();

            LocalFree(Some(HLOCAL(buffer.as_ptr() as *mut _)));

            Ok(message_string)
        }
    }

    fn get_format_message_error(original_error_code: WIN32_ERROR) -> String {
        format!("FormatMessageW failed while formatting error 0x{:08X}", original_error_code.0)
    }

    const UNKNOWN_ERROR: &'static str = "UNKNOWN ERROR CODE";
}

impl fmt::Display for WinError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(comment) = self.comment.as_ref() {
            write!(f, "{}; ", comment)?;
        }
        let message = match self.message.as_ref() {
            Some(msg) => msg.as_str(),
            None => Self::UNKNOWN_ERROR,
        };
        write!(f, "Error Code 0x{:08X}: {}", self.error_code.0, message)?;

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
