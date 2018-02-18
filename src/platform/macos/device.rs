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
#![allow(unused_variables)]

use std::mem;
use std::ptr;
use std::ffi::CStr;
use std::sync::Arc;
use std::net::Ipv4Addr;
use std::io::{self, Read, Write};
use std::os::unix::io::AsRawFd;

use libc;
use libc::{SOCK_DGRAM, AF_INET, socklen_t, sockaddr, c_void, c_char, c_uint};

use nix::unistd::{close, read, write};
use std::os::unix::io::RawFd;
use nix::sys::socket::{AddressFamily, SockType, SockAddr as NixSockAddr, Shutdown, MsgFlags, socket, connect, shutdown, send, recv, SYSPROTO_CONTROL, SOCK_NONBLOCK};

use error::Error;
use device::Device as D;
use platform::macos::sys::*;
use configuration::Configuration;
use platform::posix::{self, SockAddr, Fd};

/// A TUN device using the TUN macOS driver.
pub struct Device {
	name: String,
	tun: Fd,
	ctl: Fd,
}

impl Device {
	/// Create a new `Device` for the given `Configuration`.
	pub fn new(config: &Configuration) -> Result<Self, Error> {
        let name: String = config.name.clone().unwrap_or_else(|| "utun0".into());
        if name.len() > IFNAMSIZ {
            return Err(Error::NameTooLong);
        }

        if !name.starts_with("utun") {
            return Err(Error::InvalidName);
        }

        let id: u32 = name[4..].parse()?;

        let tun: RawFd = socket(AddressFamily::System,
                               SockType::Datagram,
                               SOCK_NONBLOCK,
                               SYSPROTO_CONTROL)?;

        let addr = NixSockAddr::new_sys_control(tun, "com.apple.net.utun_control", id + 1)?;

        connect(tun, &addr)?;

//        let mut name = [0u8; 64];
//        let mut name_len: socklen_t = 64;
//
//        if libc::getsockopt(tun.0, SYSPROTO_CONTROL, UTUN_OPT_IFNAME, &mut name as *mut _ as *mut c_void, &mut name_len as *mut socklen_t) < 0 {
//            return Err(io::Error::last_os_error().into());
//        }

        let ctl = unsafe {
            Fd::new(libc::socket(AF_INET, SOCK_DGRAM, 0))
                .map_err(|_| io::Error::last_os_error())?
        };

        let mut device = Device {
            name: name.into(),
            tun: Fd(tun),
            ctl,
        };

		device.configure(&config)?;

		Ok(device)
	}

	/// Prepare a new request.
	pub unsafe fn request(&self) -> ifreq {
		let mut req: ifreq = mem::zeroed();
		ptr::copy_nonoverlapping(self.name.as_ptr() as *const c_char, req.ifrn.name.as_mut_ptr(), self.name.len());

		req
	}

	/// Set the IPv4 alias of the device.
	pub fn set_alias(&mut self, addr: Ipv4Addr, broadaddr: Ipv4Addr, mask: Ipv4Addr) -> Result<(), Error> {
		unsafe {
			let mut req: ifaliasreq = mem::zeroed();
			ptr::copy_nonoverlapping(self.name.as_ptr() as *const c_char, req.ifran.as_mut_ptr(), self.name.len());

			req.addr = SockAddr::from(addr).into();
			req.broadaddr = SockAddr::from(broadaddr).into();
			req.mask = SockAddr::from(mask).into();

			if siocaifaddr(self.ctl.as_raw_fd(), &req) < 0 {
				return Err(io::Error::last_os_error().into());
			}

			Ok(())
		}
	}

	/// Split the interface into a `Reader` and `Writer`.
	pub fn split(self) -> (posix::Reader, posix::Writer) {
		let fd = Arc::new(self.tun);
		(posix::Reader(fd.clone()), posix::Writer(fd.clone()))
	}
}

impl Read for Device {
	fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
		self.tun.read(buf)
	}
}

impl Write for Device {
	fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
		self.tun.write(buf)
	}

	fn flush(&mut self) -> io::Result<()> {
		self.tun.flush()
	}
}

impl D for Device {
	fn name(&self) -> &str {
		&self.name
	}

	// XXX: Cannot set interface name on Darwin.
	fn set_name(&mut self, value: &str) -> Result<(), Error> {
		Err(Error::InvalidName)
	}

	fn enabled(&mut self, value: bool) -> Result<(), Error> {
		unsafe {
			let mut req = self.request();

			if siocgifflags(self.ctl.as_raw_fd(), &mut req) < 0 {
				return Err(io::Error::last_os_error().into());
			}

			if value {
				req.ifru.flags |= IFF_UP | IFF_RUNNING;
			}
			else {
				req.ifru.flags &= !IFF_UP;
			}

			if siocsifflags(self.ctl.as_raw_fd(), &req) < 0 {
				return Err(io::Error::last_os_error().into());
			}

			Ok(())
		}
	}

	fn address(&self) -> Result<Ipv4Addr, Error> {
		unsafe {
			let mut req = self.request();

			if siocgifaddr(self.ctl.as_raw_fd(), &mut req) < 0 {
				return Err(io::Error::last_os_error().into());
			}

			SockAddr::new(&req.ifru.addr).map(Into::into)
		}
	}

	fn set_address(&mut self, value: Ipv4Addr) -> Result<(), Error> {
		unsafe {
			let mut req = self.request();
			req.ifru.addr = SockAddr::from(value).into();

			if siocsifaddr(self.ctl.as_raw_fd(), &req) < 0 {
				return Err(io::Error::last_os_error().into());
			}

			Ok(())
		}
	}

	fn destination(&self) -> Result<Ipv4Addr, Error> {
		unsafe {
			let mut req = self.request();

			if siocgifdstaddr(self.ctl.as_raw_fd(), &mut req) < 0 {
				return Err(io::Error::last_os_error().into());
			}

			SockAddr::new(&req.ifru.dstaddr).map(Into::into)
		}
	}

	fn set_destination(&mut self, value: Ipv4Addr) -> Result<(), Error> {
		unsafe {
			let mut req = self.request();
			req.ifru.dstaddr = SockAddr::from(value).into();

			if siocsifdstaddr(self.ctl.as_raw_fd(), &req) < 0 {
				return Err(io::Error::last_os_error().into());
			}

			Ok(())
		}
	}

	fn broadcast(&self) -> Result<Ipv4Addr, Error> {
		unsafe {
			let mut req = self.request();

			if siocgifbrdaddr(self.ctl.as_raw_fd(), &mut req) < 0 {
				return Err(io::Error::last_os_error().into());
			}

			SockAddr::new(&req.ifru.broadaddr).map(Into::into)
		}
	}

	fn set_broadcast(&mut self, value: Ipv4Addr) -> Result<(), Error> {
		unsafe {
			let mut req = self.request();
			req.ifru.broadaddr = SockAddr::from(value).into();

			if siocsifbrdaddr(self.ctl.as_raw_fd(), &req) < 0 {
				return Err(io::Error::last_os_error().into());
			}

			Ok(())
		}
	}

	fn netmask(&self) -> Result<Ipv4Addr, Error> {
		unsafe {
			let mut req = self.request();

			if siocgifnetmask(self.ctl.as_raw_fd(), &mut req) < 0 {
				return Err(io::Error::last_os_error().into());
			}

			SockAddr::unchecked(&req.ifru.addr).map(Into::into)
		}
	}

	fn set_netmask(&mut self, value: Ipv4Addr) -> Result<(), Error> {
		unsafe {
			let mut req = self.request();
			req.ifru.addr = SockAddr::from(value).into();

			if siocsifnetmask(self.ctl.as_raw_fd(), &req) < 0 {
				return Err(io::Error::last_os_error().into());
			}

			Ok(())
		}
	}

	fn mtu(&self) -> Result<i32, Error> {
		unsafe {
			let mut req = self.request();

			if siocgifmtu(self.ctl.as_raw_fd(), &mut req) < 0 {
				return Err(io::Error::last_os_error().into());
			}

			Ok(req.ifru.mtu)
		}
	}

	fn set_mtu(&mut self, value: i32) -> Result<(), Error> {
		unsafe {
			let mut req = self.request();
			req.ifru.mtu = value;

			if siocsifmtu(self.ctl.as_raw_fd(), &req) < 0 {
				return Err(io::Error::last_os_error().into());
			}

			Ok(())
		}
	}
}

#[cfg(feature = "mio")]
mod mio {
	use std::io;
	use mio::{Ready, Poll, PollOpt, Token};
	use mio::event::Evented;

	impl Evented for super::Device {
		fn register(&self, poll: &Poll, token: Token, interest: Ready, opts: PollOpt) -> io::Result<()> {
			self.tun.register(poll, token, interest, opts)
		}

		fn reregister(&self, poll: &Poll, token: Token, interest: Ready, opts: PollOpt) -> io::Result<()> {
			self.tun.reregister(poll, token, interest, opts)
		}

		fn deregister(&self, poll: &Poll) -> io::Result<()> {
			self.tun.deregister(poll)
		}
	}
}
#[cfg(feature = "mio")] pub use mio::Evented;

#[cfg(feature = "tokio")]
pub mod tokio {
    use super::*;
	use futures::Poll;
	use tokio_core::reactor::{PollEvented, Handle};
	use tokio_io::{AsyncRead, AsyncWrite};

	pub struct Device {
		io: PollEvented<super::Device>
	}

	impl Device {
        pub fn new(config: &Configuration, handle: &Handle) -> Result<Self, Error> {
            let io = PollEvented::new(super::Device::new(config)?, handle)?;
            Ok(Self { io })
        }

		fn from_device(device: super::Device, handle: &Handle) -> io::Result<Self> {
			let io = PollEvented::new(device, handle)?;
			Ok(Self { io })
		}
	}

	impl Read for Device {
		fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
			self.io.read(buf)
		}
	}

	impl Write for Device {
		fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
			self.io.write(buf)
		}

		fn flush(&mut self) -> io::Result<()> {
			self.io.flush()
		}
	}

	impl AsyncRead for Device {}
	impl AsyncWrite for Device {
		fn shutdown(&mut self) -> Poll<(), io::Error> {
			unimplemented!()
		}
	}
}
