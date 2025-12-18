# 🔐 Credentials Setup Guide

Complete guide for obtaining and configuring Polymarket credentials.

---

## ✅ What You Need

You need **TWO** things from the **SAME** MetaMask wallet:

1. **Private Key** (`POLY_PRIVATE_KEY`) - Ethereum private key
2. **Wallet Address** (`POLY_FUNDER`) - Your wallet address on **Polygon network**

**Important**: Both must be from the **same wallet**, and the wallet must be on **Polygon network** (chain ID 137).

---

## 📋 Step-by-Step: Getting Credentials from MetaMask

### Step 1: Open MetaMask

1. Open MetaMask browser extension or mobile app
2. Make sure you're logged in

### Step 2: Switch to Polygon Network

**Option A: Add Polygon Network (if not already added)**

1. Click network dropdown (top of MetaMask)
2. Click "Add Network" or "Add a network manually"
3. Enter these details:
   - **Network Name**: Polygon Mainnet
   - **RPC URL**: `https://polygon-rpc.com`
   - **Chain ID**: `137`
   - **Currency Symbol**: `MATIC`
   - **Block Explorer**: `https://polygonscan.com`

4. Click "Save"

**Option B: Use Existing Polygon Network**

1. Click network dropdown
2. Select "Polygon Mainnet"

### Step 3: Get Your Wallet Address (`POLY_FUNDER`)

1. In MetaMask, click on your account name (top of MetaMask)
2. Click "Copy address to clipboard"
3. This is your `POLY_FUNDER` value
4. Format: `0x` + 40 hex characters (e.g., `0x56109c3E830a00EcceDB16DD2F7b65ffDaCd7962`)

### Step 4: Export Private Key (`POLY_PRIVATE_KEY`)

⚠️ **SECURITY WARNING**: Your private key gives full control of your wallet. Never share it!

1. In MetaMask, click the **three dots** (⋮) next to your account name
2. Click **"Account details"**
3. Click **"Export Private Key"**
4. Enter your MetaMask password
5. Click to reveal the private key
6. **Copy the private key** (it should start with `0x`)
7. This is your `POLY_PRIVATE_KEY` value

**Format**: `0x` + 64 hex characters (e.g., `0x8b3bcd5d61e5e43e00a7b78fb2590e1c26d9ee739e30550cece91c35dd29ee62`)

---

## 💰 Funding Your Wallet

### Required: USDC on Polygon

Polymarket uses **USDC** (not MATIC) for trading. You need USDC on Polygon network.

**How to Get USDC on Polygon:**

1. **Bridge USDC from Ethereum**:
   - Use [Polygon Bridge](https://wallet.polygon.technology/polygon/bridge)
   - Bridge USDC from Ethereum mainnet to Polygon

2. **Buy USDC Directly on Polygon**:
   - Use exchanges like Coinbase, Binance, etc.
   - Withdraw USDC to Polygon network
   - Send to your MetaMask address

3. **Minimum Amount**: 
   - At least $10-20 USDC recommended for testing
   - More for actual trading

**Verify Balance**:
- In MetaMask, switch to Polygon network
- You should see USDC balance
- Or check on [Polygonscan](https://polygonscan.com) - search your address

---

## 🔧 Setting Up `.env` File

### Format Requirements

**Correct Format**:
```bash
POLY_PRIVATE_KEY=0x8b3bcd5d61e5e43e00a7b78fb2590e1c26d9ee739e30550cece91c35dd29ee62
POLY_FUNDER=0x56109c3E830a00EcceDB16DD2F7b65ffDaCd7962
```

**Common Mistakes** ❌:
```bash
# WRONG - Missing 0x prefix
POLY_PRIVATE_KEY=8b3bcd5d61e5e43e00a7b78fb2590e1c26d9ee739e30550cece91c35dd29ee62

# WRONG - Extra spaces
POLY_FUNDER= 0x56109c3E830a00EcceDB16DD2F7b65ffDaCd7962    

# WRONG - Inline comments (not supported)
POLY_PRIVATE_KEY=0x... # Ethereum private key
```

### Verification Checklist

✅ **Private Key**:
- Starts with `0x`
- Exactly 66 characters total (`0x` + 64 hex chars)
- No spaces or extra characters
- Matches the wallet you're using

✅ **Wallet Address**:
- Starts with `0x`
- Exactly 42 characters total (`0x` + 40 hex chars)
- No spaces
- Matches the address shown in MetaMask on Polygon network

✅ **Wallet Match**:
- Private key derives to the same address as `POLY_FUNDER`
- Both are from the same MetaMask account
- Wallet is on Polygon network (chain ID 137)

---

## 🧪 Testing Your Credentials

### Quick Test Script

Create a test file to verify your credentials work:

```bash
# Test private key format
echo "Private key length: $(echo $POLY_PRIVATE_KEY | wc -c)"
# Should output: 67 (66 chars + newline)

# Test address format  
echo "Address length: $(echo $POLY_FUNDER | wc -c)"
# Should output: 43 (42 chars + newline)
```

### Verify Wallet Match

You can verify the private key matches the address using online tools (be careful!) or:

```bash
# Using Node.js (if you have it)
node -e "
const ethers = require('ethers');
const wallet = new ethers.Wallet('YOUR_PRIVATE_KEY');
console.log('Address:', wallet.address);
"
```

---

## 🚨 Troubleshooting Authentication Errors

### Error: `derive-api-key failed: 400 Bad Request`

**Possible Causes**:

1. **Private Key Format Wrong**
   - ❌ Missing `0x` prefix
   - ❌ Wrong length (not 66 chars)
   - ❌ Contains spaces or invalid characters

2. **Address Mismatch**
   - ❌ `POLY_FUNDER` doesn't match the private key
   - ❌ Address is from wrong network (Ethereum instead of Polygon)

3. **Wallet Not Set Up**
   - ❌ Wallet never used Polymarket before
   - ❌ Need to sign in to Polymarket website first

**Solutions**:

1. **Verify Format**:
   ```bash
   # Check .env file
   cat .env | grep POLY_PRIVATE_KEY
   cat .env | grep POLY_FUNDER
   
   # Should show:
   # POLY_PRIVATE_KEY=0x... (66 chars)
   # POLY_FUNDER=0x... (42 chars)
   ```

2. **Test on Polymarket Website**:
   - Go to [polymarket.com](https://polymarket.com)
   - Connect MetaMask (Polygon network)
   - Try to place a small test order
   - This "activates" the wallet on Polymarket

3. **Double-Check Network**:
   - In MetaMask, ensure you're on **Polygon Mainnet**
   - Chain ID should be **137**
   - Not Ethereum Mainnet (chain ID 1)

---

## 📝 Complete `.env` Example

```bash
# ============================================================================
# REQUIRED: Polymarket Credentials
# ============================================================================
# Get these from MetaMask (Polygon network)
POLY_PRIVATE_KEY=0xYOUR_66_CHARACTER_PRIVATE_KEY_HERE
POLY_FUNDER=0xYOUR_42_CHARACTER_WALLET_ADDRESS_HERE

# ============================================================================
# Bot Configuration
# ============================================================================
DRY_RUN=1
RUST_LOG=info

# Market discovery (comma-separated Polymarket slugs)
# Find these on polymarket.com URLs: polymarket.com/event/{slug}
POLY_MARKET_SLUGS=epl-che-avl-2025-12-08,epl-mci-liv-2025-12-09

# ============================================================================
# Advanced Configuration (Optional)
# ============================================================================
FORCE_DISCOVERY=0
PRICE_LOGGING=0
TEST_ARB=0
TEST_ARB_TYPE=poly_only

# ============================================================================
# Circuit Breaker Settings (Optional)
# ============================================================================
CB_ENABLED=true
CB_MAX_POSITION_PER_MARKET=100
CB_MAX_TOTAL_POSITION=500
CB_MAX_DAILY_LOSS=5000
CB_MAX_CONSECUTIVE_ERRORS=5
CB_COOLDOWN_SECS=60
```

---

## 🔒 Security Best Practices

1. **Never Commit `.env` to Git**
   - Already in `.gitignore` ✅
   - Double-check before pushing

2. **Use Separate Wallet for Trading**
   - Don't use your main wallet
   - Create a dedicated trading wallet
   - Only fund with what you need

3. **Keep Private Key Secret**
   - Never share it
   - Never paste it in public places
   - Use environment variables only

4. **Backup Securely**
   - Store private key in password manager
   - Or encrypted file
   - Never in plain text files

---

## ✅ Final Checklist

Before running the bot:

- [ ] MetaMask installed and set up
- [ ] Polygon network added to MetaMask
- [ ] Wallet switched to Polygon Mainnet (chain ID 137)
- [ ] Private key exported (66 chars, starts with `0x`)
- [ ] Wallet address copied (42 chars, starts with `0x`)
- [ ] `.env` file created with correct format
- [ ] No spaces or comments in `.env` values
- [ ] Wallet has USDC on Polygon network
- [ ] Tested connection on Polymarket website
- [ ] Verified private key matches address

---

## 🆘 Still Having Issues?

**Common Problems**:

1. **"Could not derive api key"**
   - Try connecting to Polymarket website first
   - Make sure wallet is on Polygon network
   - Verify private key format is correct

2. **"Invalid character '#'"**
   - Remove inline comments from `.env`
   - Comments should be on separate lines starting with `#`

3. **"Wallet address mismatch"**
   - Verify private key derives to the address
   - Use same MetaMask account for both

4. **"Insufficient funds"**
   - Need USDC (not MATIC) on Polygon
   - Check balance on Polygonscan

---

**Ready to trade!** Once credentials are correct, the bot will authenticate and start monitoring markets. 🚀

