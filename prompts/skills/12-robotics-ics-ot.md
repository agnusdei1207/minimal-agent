# 12. Robotics / ICS / OT

**When:** Targeting industrial control systems (ICS/SCADA), Programmable Logic Controllers (PLCs), robotic middleware (ROS), or OT network protocols.

## Mental model
Industrial and robotics protocols were architected under the assumption of **physically isolated, trusted air-gapped networks**: they virtually **lack authentication, encryption, and authorization**. If an attacker gains IP-level connectivity to a controller or topic broker, **network reachability equals complete command execution**.

## Attack arc
- **Protocol Discovery & Mapping:**
  - Scan for industrial protocol ports:
    - Modbus/TCP: Port `502`
    - Siemens S7Comm / S7CommPlus: Port `102`
    - EtherNet/IP & CIP: Port `44818` (TCP/UDP), `2222` (UDP)
    - DNP3: Port `20000`
    - OPC-UA: Port `4840`
    - BACnet: Port `47808`
    - ROS 1 (Master): Port `11311` / ROS 2 (DDS): Multicast `7400+`
  - Probe PLC device information (Vendor, model, firmware version) using Nmap NSE scripts (`nmap -sV --script "modbus-discover,s7-info" -p 102,502,44818`).
- **Modbus/TCP Manipulation:**
  - Read Holding Registers (Function Code 3) and Input Registers (Function Code 4) to monitor sensor states.
  - Write Single/Multiple Coils and Holding Registers (Function Codes 5, 6, 15, 16) using Python (`pymodbus`) to override setpoints, trip safety interlocks, or open valves.
- **Siemens S7Comm Exploitation:**
  - Read/Write Data Blocks (DB), Merkers, and I/O areas using `snap7` library in Python.
  - Start/Stop PLC CPU operations or download modified ladder logic bytecodes.
- **Robotics Middleware (ROS 1 & ROS 2):**
  - *ROS 1:* Connect to Master (`ROS_MASTER_URI=http://<target>:11311`), enumerate nodes, services, and topics (`rostopic list`). Publish malicious trajectory/velocity commands (`rostopic pub /cmd_vel ...`).
  - *ROS 2:* Sniff and inject DDS multicast messages without authentication; manipulate sensor feedback loops.
- **HMI & Workstation Exploitation:**
  - Access web-based Human-Machine Interfaces (HMIs) with default credentials (`admin:admin`, `operator:operator`).
  - Exploit Engineering Workstation (EWS) software running on Windows systems (link with Windows/AD card).

## Key techniques & primitives
- **Process Feedback Spoofing:** Suppress alarm conditions by continuously overwriting register values while driving physical actuators past safe limits.
- **Pymodbus Scripting:**
  `from pymodbus.client import ModbusTcpClient; client = ModbusTcpClient('TARGET'); client.write_register(address=100, value=0xDEAD)`.

## Tells & signals
- Unauthenticated Modbus/S7/DNP3 ports exposed without firewall ACLs.
- Cleartext engineering credentials in SCADA network captures.
- ROS master accepting remote XML-RPC node registrations without TLS.
