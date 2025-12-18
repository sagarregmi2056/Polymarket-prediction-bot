# 🔧 Troubleshooting Guide

Common errors and solutions for the Polymarket Arbitrage Bot.

---

## ❌ Error: `derive-api-key failed: 400 Bad Request {"error":"Could not derive api key!"}`

This error occurs during authentication with Polymarket's CLOB API. The bot is trying to derive API credentials but Polymarket is rejecting the request.

### 🔍 Root Causes

#### 1. **Private Key Format Issue** (Most Common)

**Problem**: Private key is malformed or incorrect format.

**Symptoms**:
- Error occurs immediately after "Creating async client"
- Private key doesn't parse correctly

**Solutions**:
```bash
# Check private key format
echo $POLY_PRIVATE_KEY | wc -c
# Should be 67 (66 chars + newline)

# Verify it starts with 0x
echo $POLY_PRIVATE_KEY | head -c 2
# Should output: 0x

# Check for invalid characters
echo $POLY_PRIVATE_KEY | grep -E '^0x[0-9a-fA-F]{64}$'
# Should match the entire key
```

**Fix**:
- Ensure private key starts with `0x`
- Must be exactly 66 characters (`0x` + 64 hex chars)
- No spaces, newlines, or special characters
- Only hexadecimal characters (0-9, a-f, A-F)

---

#### 2. **Address Mismatch**

**Problem**: `POLY_FUNDER` doesn't match the address derived from `POLY_PRIVATE_KEY`.

**How It Works**:
- The bot derives the wallet address from your private key
- This derived address is used for authentication
- If `POLY_FUNDER` doesn't match, there's a mismatch

**Check**:
```bash
# The bot derives address from private key automatically
# POLY_FUNDER should match the address shown in MetaMask
# Both should be from the same wallet
```

**Fix**:
- Use the **same** MetaMask account for both
- Copy address directly from MetaMask (don't type manually)
- Verify address matches what MetaMask shows

---

#### 3. **Wrong Network**

**Problem**: Wallet is on Ethereum mainnet instead of Polygon.

**Symptoms**:
- Wallet works in MetaMask
- But authentication fails

**Check**:
- In MetaMask, verify you're on **Polygon Mainnet**
- Chain ID should be **137** (not 1 for Ethereum)
- Address should be the same on both networks, but network matters!

**Fix**:
1. Switch MetaMask to Polygon Mainnet
2. Copy address from Polygon network
3. Ensure private key is from the same account
4. Update `.env` with Polygon address

---

#### 4. **Wallet Never Used Polymarket**

**Problem**: Wallet exists but has never interacted with Polymarket.

**Symptoms**:
- Credentials are correct format
- Wallet has funds
- But API rejects authentication

**Why**: Polymarket may require initial "activation" by connecting via their website first.

**Fix**:
1. Go to [polymarket.com](https://polymarket.com)
2. Click "Connect Wallet"
3. Select MetaMask
4. **Ensure MetaMask is on Polygon network**
5. Approve connection
6. Try placing a small test order (optional, but helps)
7. Then run the bot again

---

#### 5. **Private Key Doesn't Match Address**

**Problem**: Private key and address are from different wallets.

**How to Verify**:
```bash
# Using Node.js (if available)
node -e "
const ethers = require('ethers');
const wallet = new ethers.Wallet('YOUR_PRIVATE_KEY');
console.log('Derived address:', wallet.address);
console.log('Your POLY_FUNDER:', 'YOUR_POLY_FUNDER');
console.log('Match:', wallet.address.toLowerCase() === 'YOUR_POLY_FUNDER'.toLowerCase());
"
```

**Fix**:
- Export private key from the **exact same** MetaMask account
- Don't mix accounts
- Double-check you copied the right address

---

#### 6. **Signature Format Issue**

**Problem**: The EIP712 signature format is incorrect.

**Less Common**: Usually means there's an issue with how the wallet signs.

**Check**:
- Ensure you're using a standard Ethereum wallet (MetaMask)
- Not a hardware wallet with special signing requirements
- Private key format is correct

---

### 🔍 Debugging Steps

#### Step 1: Verify .env Format

```bash
cd /path/to/poly-kalshi-arb
cat .env | grep POLY_PRIVATE_KEY
cat .env | grep POLY_FUNDER

# Should show:
# POLY_PRIVATE_KEY=0x... (66 chars, no spaces)
# POLY_FUNDER=0x... (42 chars, no spaces)
```

#### Step 2: Check for Hidden Characters

```bash
# Check for trailing spaces or newlines
echo -n "$POLY_PRIVATE_KEY" | wc -c  # Should be 66
echo -n "$POLY_FUNDER" | wc -c       # Should be 42

# Check for invalid characters
echo "$POLY_PRIVATE_KEY" | od -c | head -5
```

#### Step 3: Test Wallet Connection

1. Open MetaMask
2. Switch to Polygon Mainnet
3. Go to [polymarket.com](https://polymarket.com)
4. Click "Connect Wallet"
5. Select MetaMask
6. Approve connection
7. Verify it connects successfully

If this fails, the issue is with your wallet setup, not the bot.

#### Step 4: Verify Address Match

The bot logs should show the derived address. Check if it matches:

```bash
# Run bot and look for address in logs
docker run --rm --env-file .env polymarket-bot 2>&1 | grep -i address
```

---

### 🛠️ Common Fixes

#### Fix 1: Clean .env File

```bash
# Remove all comments and whitespace
sed -i 's/#.*$//' .env           # Remove comments
sed -i 's/^[[:space:]]*//' .env  # Remove leading spaces
sed -i 's/[[:space:]]*$//' .env  # Remove trailing spaces
sed -i '/^$/d' .env              # Remove empty lines

# Verify
cat .env
```

#### Fix 2: Add Missing 0x Prefix

```bash
# Add 0x to private key if missing
sed -i 's/^POLY_PRIVATE_KEY=\(.*\)$/POLY_PRIVATE_KEY=0x\1/' .env

# Verify
grep POLY_PRIVATE_KEY .env
```

#### Fix 3: Fix Address Format

```bash
# Remove spaces from address
sed -i 's/^POLY_FUNDER= *\(.*\)$/POLY_FUNDER=\1/' .env

# Verify
grep POLY_FUNDER .env
```

#### Fix 4: Test with Minimal .env

Create a test `.env` with only required fields:

```bash
cat > .env.test << EOF
POLY_PRIVATE_KEY=0xYOUR_KEY_HERE
POLY_FUNDER=0xYOUR_ADDRESS_HERE
DRY_RUN=1
RUST_LOG=info
EOF

# Test with this file
docker run --rm --env-file .env.test polymarket-bot
```

---

### 📊 Error Analysis

The error `400 Bad Request` from Polymarket API means:

1. **Request Format**: The HTTP request is malformed
2. **Authentication**: The signature/credentials are invalid
3. **Validation**: Polymarket rejected the authentication attempt

**What Polymarket Checks**:
- ✅ EIP712 signature is valid
- ✅ Address matches the signature
- ✅ Chain ID is correct (137 for Polygon)
- ✅ Timestamp is recent
- ✅ Nonce is valid
- ✅ Wallet has been used on Polymarket (sometimes)

---

### ✅ Verification Checklist

Before reporting the error, verify:

- [ ] Private key is exactly 66 characters (`0x` + 64 hex)
- [ ] Private key starts with `0x`
- [ ] Address is exactly 42 characters (`0x` + 40 hex)
- [ ] Address starts with `0x`
- [ ] No spaces in `.env` values
- [ ] No inline comments in `.env` values
- [ ] Both are from same MetaMask account
- [ ] MetaMask is on Polygon Mainnet (chain ID 137)
- [ ] Wallet has been connected to Polymarket website
- [ ] Wallet has USDC on Polygon (for actual trading)

---

### 🆘 Still Not Working?

If all checks pass but still getting the error:

1. **Try Different Wallet**:
   - Create a new MetaMask wallet
   - Fund it with USDC on Polygon
   - Connect to Polymarket website
   - Use those credentials

2. **Check Polymarket Status**:
   - Visit [status.polymarket.com](https://status.polymarket.com)
   - Check if API is operational

3. **Contact Support**:
   - Polymarket Discord: [discord.gg/polymarket](https://discord.gg/polymarket)
   - Check their documentation: [docs.polymarket.com](https://docs.polymarket.com)

4. **Enable Debug Logging**:
   ```bash
   RUST_LOG=debug docker run --rm --env-file .env polymarket-bot
   ```
   This shows more detailed error messages.

---

## 🔍 Other Common Errors

### Error: `POLY_PRIVATE_KEY not set`

**Fix**: Ensure `.env` file exists and has `POLY_PRIVATE_KEY=...`

### Error: `invalid character '#' at position X`

**Fix**: Remove inline comments from `.env` file. Comments should be on separate lines.

### Error: `No market pairs found!`

**Fix**: Set `POLY_MARKET_SLUGS` environment variable with comma-separated market slugs.

### Error: WebSocket connection failed

**Fix**: Check internet connection and firewall settings. Ensure port 443 (WSS) is open.

---

**Need more help?** Check the [Credentials Guide](./CREDENTIALS_GUIDE.md) for detailed setup instructions.

