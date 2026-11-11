//! How to run jackd, and its command line.

/// How to run jackd: Dexter's `JackConfig`, its fields as they are.
#[derive(Clone, Debug, PartialEq)]
pub struct Settings {
    pub driver: String,
    pub device: String,
    pub rate: i64,
    pub period: i64,
    pub periods: i64,
    pub input_latency: i64,
    pub output_latency: i64,
    pub realtime: bool,
    pub server_args: Vec<String>,
    pub driver_args: Vec<String>,
}

impl Default for Settings {
    fn default() -> Settings {
        Settings {
            driver: "alsa".into(),
            device: "hw:0".into(),
            rate: 48000,
            period: 512,
            periods: 2,
            input_latency: 0,
            output_latency: 0,
            realtime: true,
            server_args: Vec::new(),
            driver_args: Vec::new(),
        }
    }
}

impl Settings {
    /// jackd's command line for the server named server: `-n` only when it isn't `default`.
    pub fn args(&self, server: &str) -> Vec<String> {
        let mut args = vec!["jackd".to_string()];
        if !server.is_empty() && server != "default" {
            args.push("-n".into());
            args.push(server.into());
        }
        args.push(if self.realtime { "-R" } else { "-r" }.into());
        args.extend(self.server_args.iter().cloned());
        args.extend(["-d".to_string(), self.driver.clone()]);
        let alsa = self.driver == "alsa";
        // Other drivers have no device, or take it differently.
        if alsa && !self.device.is_empty() {
            args.extend(["-d".to_string(), self.device.clone()]);
        }
        args.extend(["-r".to_string(), self.rate.to_string(), "-p".to_string(), self.period.to_string()]);
        if alsa {
            if self.periods > 0 {
                args.extend(["-n".to_string(), self.periods.to_string()]);
            }
            if self.input_latency != 0 {
                args.extend(["-I".to_string(), self.input_latency.to_string()]);
            }
            if self.output_latency != 0 {
                args.extend(["-O".to_string(), self.output_latency.to_string()]);
            }
        }
        args.extend(self.driver_args.iter().cloned());
        args
    }

    /// The same on JACK's dummy driver, at the same rate and period, with captures inputs and
    /// playbacks outputs. How many is the caller's: Dexter counts a project's tracks.
    pub fn on_dummy(&self, captures: usize, playbacks: usize) -> Settings {
        Settings {
            driver: "dummy".into(),
            device: String::new(),
            input_latency: 0,
            output_latency: 0,
            driver_args: vec!["-C".into(), captures.to_string(), "-P".into(), playbacks.to_string()],
            ..self.clone()
        }
    }
}

/// What to start: the settings, the interface by its id (`as_interfaces`), found at its `hw:N`
/// when JACK starts, and the commands run first (the interface's own setup).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Start {
    pub settings: Settings,
    pub interface: Option<String>,
    pub setup: Vec<String>,
}

impl Start {
    /// Whether it names an interface on ALSA that isn't plugged in: the caller may then start the
    /// same on the dummy driver, as Dexter does for a project.
    pub fn interface_missing(&self) -> bool {
        self.interface
            .as_deref()
            .filter(|_| self.settings.driver == "alsa")
            .is_some_and(|id| as_interfaces::resolve(std::path::Path::new("/"), id).is_err())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn args() {
        let alsa = Settings {
            driver: "alsa".into(),
            device: "hw:USB".into(),
            rate: 48000,
            period: 512,
            periods: 3,
            input_latency: 46,
            output_latency: 46,
            realtime: true,
            server_args: vec![],
            driver_args: vec![],
        };
        assert_eq!(
            alsa.args("default").join(" "),
            "jackd -R -d alsa -d hw:USB -r 48000 -p 512 -n 3 -I 46 -O 46"
        );
        assert_eq!(
            alsa.args("rig").join(" "),
            "jackd -n rig -R -d alsa -d hw:USB -r 48000 -p 512 -n 3 -I 46 -O 46"
        );
        // The built-in default device (hw:0) must not reach a driver without one.
        let dummy = Settings {
            driver: "dummy".into(),
            driver_args: vec!["-C".into(), "4".into()],
            ..Settings::default()
        };
        assert_eq!(dummy.args("default").join(" "), "jackd -R -d dummy -r 48000 -p 512 -C 4");
    }

    #[test]
    fn on_dummy() {
        let s = Settings { rate: 44100, period: 128, input_latency: 40, ..Settings::default() };
        let d = s.on_dummy(12, 6);
        assert!(d.args("default").join(" ").ends_with("-d dummy -r 44100 -p 128 -C 12 -P 6"));
        assert_eq!((d.input_latency, d.realtime), (0, true));
    }

    #[test]
    fn interface_missing() {
        let mut s = Start { interface: Some("usb-Nobody_Here_0-00".into()), ..Start::default() };
        assert!(s.interface_missing());
        s.settings.driver = "dummy".into();
        assert!(!s.interface_missing(), "only on ALSA");
        s.interface = None;
        assert!(!s.interface_missing());
    }
}
