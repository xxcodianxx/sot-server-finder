use std::{net::IpAddr, process::Command};

use etherparse::{IpHeader, PacketHeaders};
use sysinfo::{PidExt, ProcessExt, ProcessRefreshKind, RefreshKind, System, SystemExt};

mod wordlist;

fn get_sot_pid(s: &System) -> Option<u32> {
    for process in s.processes_by_name("SoTGame.exe") {
        return Some(process.pid().as_u32());
    }

    None
}

fn get_sot_ports(pid: u32) -> Vec<u16> {
    let p = &pid.to_string();

    let cmd = Command::new("netstat")
        .arg("-anop")
        .arg("udp")
        .output()
        .unwrap();

    String::from_utf8(cmd.stdout)
        .unwrap()
        .lines()
        .filter(|line| line.contains(p))
        .map(|f| {
            let addr = f.split_whitespace().skip(1).next().unwrap();
            let port = addr.split(':').last().unwrap();
            port.parse::<u16>().unwrap()
        })
        .collect()
}

fn main() {
    // wait until we get a sot pid
    println!("Looking for Sea of Thieves...");
    let mut s =
        System::new_with_specifics(RefreshKind::new().with_processes(ProcessRefreshKind::new()));

    let sot_pid = loop {
        if let Some(pid) = get_sot_pid(&s) {
            break pid;
        }
        s.refresh_processes();
    };

    println!("Found! PID: {}", sot_pid);

    let devices = pcap::Device::list().unwrap();
    let auto_found_dev = devices.iter().find(|d| {
        d.addresses.iter().any(|addr| {
            if let IpAddr::V4(addr) = addr.addr {
                addr.octets()[0] == 192 && addr.octets()[1] == 168
            } else {
                false
            }
        })
    });

    let dev = match auto_found_dev {
        Some(d) => d.clone(),
        None => {
            println!("Couldn't guess which network adapter to use. Please select one manually.");
            println!("Network adapters attached to your PC: ");

            let devices = pcap::Device::list().expect("device lookup failed");
            let mut i = 1;

            for device in devices.clone() {
                println!(
                    "    {i}. {:?}",
                    device.desc.clone().unwrap_or(device.name.clone())
                );
                i += 1;
            }

            // prompt user for their device
            println!(
                "Please select your WiFi or Ethernet card, or if you're on a VPN, select the VPN: "
            );
            let mut input = String::new();
            std::io::stdin().read_line(&mut input).unwrap();
            let n = input.trim().parse::<usize>().unwrap() - 1;

            (&devices[n]).clone()
        }
    };

    println!("Waiting for you to connect to a game in Sea of Thieves...");
    let mut cap = pcap::Capture::from_device(dev)
        .unwrap()
        .immediate_mode(true)
        .open()
        .unwrap();

    // iterate udp packets
    loop {
        if let Ok(raw_packet) = cap.next_packet() {
            if let Ok(packet) = PacketHeaders::from_ethernet_slice(raw_packet.data) {
                if let Some(IpHeader::Version4(ipv4, _)) = packet.ip {
                    if let Some(transport) = packet.transport {
                        if let Some(udp) = transport.udp() {
                            if udp.destination_port == 3075 || udp.destination_port == 30005 {
                                continue;
                            }

                            if get_sot_ports(sot_pid).contains(&udp.source_port) {
                                let friendly = ipv4
                                    .destination
                                    .map(|x| wordlist::WORDS[x as usize].to_string())
                                    .join("-");

                                println!(
                                    "You are connected to: {} ({}:{})",
                                    friendly,
                                    ipv4.destination.map(|c| c.to_string()).join("."),
                                    udp.destination_port
                                );

                                // wait for enter to be hit
                                std::io::stdin().read_line(&mut String::new()).unwrap();
                            }
                        }
                    }
                }
            }
        }
    }
}