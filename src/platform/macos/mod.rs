//            DO WHAT THE FUCK YOU WANT TO PUBLIC LICENSE
//                    Version 2, December 2004
//
// Copyleft (ↄ) meh. <meh@schizofreni.co> | http://meh.schizofreni.co
//
// Everyone is permitted to copy and distribute verbatim or modified
// copies of this license document, and changing it is allowed as long
// as the name is changed.
//
//            DO WHAT THE FUCK YOU WANT TO PUBLIC LICENSE
//   TERMS AND CONDITIONS FOR COPYING, DISTRIBUTION AND MODIFICATION
//
//  0. You just DO WHAT THE FUCK YOU WANT TO.

//! macOS specific functionality.

pub mod sys;

mod device;
pub use self::device::Device;
#[cfg(feature = "tokio")]
pub use self::device::tokio;

use configuration::Configuration as C;
use error::Error;

/// macOS-only interface configuration.
#[derive(Copy, Clone, Default, Debug)]
pub struct Configuration { }

/// Create a TUN device with the given name.
pub fn create(configuration: &C) -> Result<Device, Error> {
	Device::new(&configuration)
}

#[cfg(feature = "tokio")]
pub fn create_tokio(configuration: &C, handle: &::tokio_core::reactor::Handle) -> Result<tokio::Device, Error> {
	tokio::Device::new(&configuration, handle)
}
