# 🚀 Universal Blockchain Network Deployment Guide

This guide walks you through deploying the Universal Blockchain Network with its revolutionary backend systems and AuraProtocol smart contract.

## 📋 Prerequisites

1. **Node.js 16+** installed
2. **Ronin wallet** with testnet RON for gas fees
3. **Private key** from your wallet for deployment

## ⚡ Quick Setup

### 1. Install Dependencies

```bash
npm install
```

### 2. Configure Environment

Copy the environment template and fill in your values:

```bash
cp .env.example .env
```

Edit `.env` and add your private key:

```env
# REQUIRED: Your wallet private key (without 0x prefix)
PRIVATE_KEY=your_private_key_here

# OPTIONAL: For production, set the NXS token address
NXS_TOKEN_ADDRESS=0x0000000000000000000000000000000000000000
```

### 3. Get Testnet RON

1. Visit [Ronin Faucet](https://faucet.roninchain.com/)
2. Connect your wallet
3. Request testnet RON (you need ~0.1 RON for deployment)

## 🚀 Deployment

### Deploy to Ronin Testnet

```bash
npm run deploy:testnet
```

Expected output:
```
🚀 Starting AuraProtocol deployment to Ronin network...
📝 Deploying contracts with account: 0x...
💰 Account balance: 1.0 RON
✅ AuraProtocol deployed successfully!
📍 Contract Address: 0x1234567890123456789012345678901234567890
```

### Deploy to Ronin Mainnet (Production)

⚠️ **Only for production use**

```bash
npm run deploy:mainnet
```

## 📄 After Deployment

### 1. Verify Contract (Optional)

```bash
npm run verify 0xYourContractAddress
```

### 2. Update Rust Configuration

The deployment script automatically updates `Settings.toml` with the contract address:

```toml
# AuraProtocol contract address is now set
aura_protocol_address = "0x1234567890123456789012345678901234567890"
```

### 3. Test the Integration

Start the Universal Blockchain Network:

```bash
cargo run
```

The system should now connect to your deployed contract with all revolutionary features active:
- ✅ Hidden banking system operational
- ✅ Cross-chain bridge ready
- ✅ Distributed computing network active
- ✅ Engine shell encryption protecting all systems

## 🔧 Configuration

### Contract Parameters

The contract is deployed with these default settings:

- **Verification Quorum**: 66% (2/3 majority required)
- **Initial Admin**: Your deployment address
- **NXS Token**: Zero address (for testing)

### Network Settings

| Network | Chain ID | RPC URL |
|---------|----------|---------|
| Ronin Testnet | 2021 | https://saigon-testnet.roninchain.com/rpc |
| Ronin Mainnet | 2020 | https://api.roninchain.com/rpc |

## 🛠️ Troubleshooting

### Common Issues

**"Insufficient funds"**
- Get more testnet RON from the faucet
- Check your wallet balance

**"Nonce too high/low"**
- Reset your wallet nonce in MetaMask
- Wait a few minutes and try again

**"Gas estimation failed"**
- Check network connectivity
- Verify contract code compiles: `npm run compile`

**"Private key not found"**
- Make sure `.env` file exists
- Check private key format (no 0x prefix)

### Getting Help

1. Check the deployment logs for specific error messages
2. Verify your `.env` configuration
3. Ensure you have sufficient testnet RON
4. Try compiling first: `npm run compile`

## 📊 Deployment Information

After successful deployment, check these files:

- `deployment.json` - Complete deployment details
- `Settings.toml` - Updated with contract address
- Console output - Contract address and transaction hash

## 🎯 Next Steps

1. **Grant Roles**: Use admin functions to grant roles to mesh nodes
2. **Test Integration**: Start the Rust engine and verify contract interaction
3. **Monitor Events**: Watch for TaskCreated and other contract events
4. **Scale Network**: Deploy to multiple nodes for mesh validation

## 🔐 Security Notes

- Keep your private key secure and never commit it to version control
- Use a dedicated deployment wallet for production
- Consider using a hardware wallet for mainnet deployments
- Regularly update dependencies for security patches
