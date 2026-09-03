# 14. Wireless & RF

When: Targeting 802.11 Wi-Fi, Bluetooth/BLE, Zigbee, NFC/RFID, or Sub-GHz RF devices in physical/radio range.

## Mental model
Wireless communications use an open, uncontained shared medium: physical proximity allows any receiver to capture frames. Security is entirely dependent on cryptographic handshakes, pre-shared keys, and pairing protocols. When key exchange or state validation fails, the physical link is breached.

## Attack arc
- 802.11 Wi-Fi Exploitation:
  - *Monitor Mode & Recon:* Put interface into monitor mode (`airmon-ng start wlan0`); scan BSSIDs, channels, ESSIDs, encryption (WEP/WPA2/WPA3/Enterprise) via `airodump-ng`.
  - *WPA2 Handshake Capture:* Send deauthentication frames (`aireplay-ng -0`) to disconnect a client and capture the EAPOL 4-way handshake in a PCAP.
  - *PMKID Attack (Clientless):* Extract PMKID from the first EAPOL frame sent by the AP (`hcxdumptool` / `hcxpcapngtool`) without waiting for clients.
  - *Offline Cracking:* Crack captured handshakes/PMKID using `hashcat -m 22000` with wordlists and mutation rules.
  - *WPA Enterprise (802.1X):* Set up an Evil Twin AP with `hostapd-wpe` / `eaphammer` to capture MSCHAPv2 challenge-response hashes from authenticating corporate clients.
- Bluetooth & Bluetooth Low Energy (BLE):
  - Scan devices and discover GATT services and characteristics (`bluetoothctl`, `hcitool lescan`, `bettercap`).
  - Enumerate UUIDs for readable/writable characteristics lacking encryption or authentication (`gatttool -b <MAC> --characteristics`).
  - Read sensitive sensor data or write control bytes directly to actuate hardware.
- Sub-GHz RF & SDR (Software Defined Radio):
  - Capture raw I/Q samples using RTL-SDR / HackRF (`rtl_sdr`, `gqrx`, `urh`).
  - Demodulate signals (ASK, FSK, OOK, PSK) and inspect preamble, sync words, and payloads.
  - *Fixed Code Replay:* Transmit recorded signal directly (`hackrf_transfer`) against garage doors or gate openers lacking rolling codes.
  - *Rolling Code Analysis:* Perform Jam & Sniff attacks (Rolljam concept) to desynchronize counter windows.

## Key techniques & primitives
- PMKID Hash Extraction: `hcxdumptool -i wlan0 -o capture.pcapng --enable_status=1` followed by `hcxpcapngtool -o hash.22000 capture.pcapng`.
- GATT Characteristic Injection: `gatttool -b <MAC> --char-write-req -a <HANDLE> -n <HEX_PAYLOAD>`.

## Tells & signals
- AP configured with WEP or open guest network with internal corporate subnet access.
- BLE characteristics returning ASCII strings or status byte arrays without PIN pairing.
- Static, repeatable RF waveforms in SDR waterfall display = fixed-code replay candidate.
