# 18. Smart Contracts & Web3

**When:** Auditing Ethereum/EVM smart contracts (Solidity/Vyper), decentralized applications (DApps), DeFi protocols, or EVM bytecode.

## Mental model
EVM smart contracts are **immutable state machines executed deterministically in a public environment**: all storage, memory, and transactions are visible to everyone. Vulnerabilities arise from **broken state transitions, unsafe external calls, economic oracle manipulation, and storage slot collisions**.

## Attack arc
- **EVM Execution & Storage Mapping:**
  - Understand storage layout: 32-byte slots, packed variables, dynamic arrays/mappings mapped via `keccak256(key || slot)`.
  - Contrast execution contexts: `call` (callee storage/context), `delegatecall` (caller storage/context with callee code), `staticcall` (read-only).
- **Core Smart Contract Vulnerability Classes:**
  - *Reentrancy:* External token or ETH transfers (e.g. `call{value: amt}("")`) handing control back to an attacker before state updates (violating Checks-Effects-Interactions). Also analyze *Read-Only Reentrancy* where intermediate state affects external price feeds during a view call.
  - *Flash Loans & Price Oracle Manipulation:* Borrow millions in uncollateralized capital in a single transaction; manipulate AMM spot prices (`getReserves()` on Uniswap v2/v3 pairs) to liquidate collateral or drain lending pools at artificially skewed rates.
  - *Proxy & Storage Collisions:* Transparent/UUPS upgradeable proxies where implementation variables overwrite proxy administrative slots (bypassing EIP-1967 standards).
  - *Unsafe Delegatecall:* User-controlled `delegatecall` target allowing arbitrary storage modification (e.g. overwriting the `owner` variable at slot 0).
  - *Access Control & Logic:* Using `tx.origin` instead of `msg.sender` for authentication (allowing phishing via malicious contracts), uninitialized implementation contracts (`initialize()` callable by anyone).
  - *Signature Replay & Malleability:* Signatures lacking `nonce`, contract address, or `chainId` (replayable across forks); ECDSA $s$-value malleability ($s > \text{secp256k1n}/2$).
  - *Arithmetic & Precision Loss:* Rounding to zero in division before multiplication (`(amount / total) * reward` instead of `(amount * reward) / total`).
- **Tooling & Exploit Engineering:**
  - Write unit-test exploits in **Foundry** (`forge test -vvvv`, `cast send`, `cast call`).
  - Perform static analysis with `slither` and interactive bytecode exploration with `chisel`.
  - Disassemble raw EVM bytecode to opcode streams (`evm disasm`, `panoramix`, `heimdall`) when source code is not verified on Etherscan.

## Key techniques & primitives
- **Foundry Forking Exploit Template:**
  ```solidity
  // test/Exploit.t.sol
  function testExploit() public {
      vm.createSelectFork("http://rpc-endpoint");
      AttackerContract attacker = new AttackerContract(target);
      attacker.pwn();
      assertEq(target.balance, 0);
  }
  ```
- **Reentrancy Fallback Function:**
  `fallback() external payable { if (address(target).balance >= 1 ether) { target.withdraw(); } }`.

## Tells & signals
- State variable updates occurring *after* an external transfer `msg.sender.call{value: ...}("")`.
- Contracts determining asset prices directly from `pair.getReserves()` or `balanceOf(pair)` without TWAP or Chainlink oracles.
- Low-level `delegatecall` invocations accepting user-supplied target addresses or calldata.
