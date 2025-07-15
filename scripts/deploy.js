const { ethers, upgrades } = require("hardhat");
const fs = require("fs");
const path = require("path");

async function main() {
  console.log("🚀 Starting AuraProtocol deployment to Ronin network...");
  
  // Get the deployer account
  const signers = await ethers.getSigners();
  const deployer = signers[0];
  console.log("📝 Deploying contracts with account:", deployer.address);
  
  // Check deployer balance
  const balance = await deployer.provider.getBalance(deployer.address);
  console.log("💰 Account balance:", ethers.formatEther(balance), "RON");
  
  if (balance < ethers.parseEther("0.1")) {
    console.warn("⚠️  Warning: Low balance. Make sure you have enough RON for deployment.");
  }

  // Deployment parameters
  const NXS_TOKEN_ADDRESS = process.env.NXS_TOKEN_ADDRESS || "0x0000000000000000000000000000000000000000";
  const INITIAL_ADMIN = deployer.address;
  const VERIFICATION_QUORUM = 66; // 66% quorum for validation

  console.log("🔧 Deployment parameters:");
  console.log("  - NXS Token Address:", NXS_TOKEN_ADDRESS);
  console.log("  - Initial Admin:", INITIAL_ADMIN);
  console.log("  - Verification Quorum:", VERIFICATION_QUORUM + "%");

  if (NXS_TOKEN_ADDRESS === "0x0000000000000000000000000000000000000000") {
    console.warn("⚠️  Warning: Using zero address for NXS token. Update NXS_TOKEN_ADDRESS in .env for production.");
  }

  try {
    // Deploy the AuraProtocol contract using OpenZeppelin upgrades
    console.log("\n📦 Deploying AuraProtocol contract...");
    
    const AuraProtocol = await ethers.getContractFactory("AuraProtocol");
    
    const auraProtocol = await upgrades.deployProxy(
      AuraProtocol,
      [NXS_TOKEN_ADDRESS, INITIAL_ADMIN, VERIFICATION_QUORUM],
      {
        initializer: "initialize",
        kind: "uups",
      }
    );

    await auraProtocol.waitForDeployment();
    const contractAddress = await auraProtocol.getAddress();

    console.log("✅ AuraProtocol deployed successfully!");
    console.log("📍 Contract Address:", contractAddress);
    console.log("🔗 Network:", (await ethers.provider.getNetwork()).name);
    console.log("⛽ Gas Used: Calculating...");

    // Get deployment transaction details
    const deployTx = auraProtocol.deploymentTransaction();
    if (deployTx) {
      const receipt = await deployTx.wait();
      console.log("⛽ Gas Used:", receipt.gasUsed.toString());
      console.log("💸 Gas Price:", ethers.formatUnits(receipt.gasPrice, "gwei"), "gwei");
      console.log("💰 Total Cost:", ethers.formatEther(receipt.gasUsed * receipt.gasPrice), "RON");
    }

    // Save deployment information
    const deploymentInfo = {
      contractAddress: contractAddress,
      network: (await ethers.provider.getNetwork()).name,
      chainId: (await ethers.provider.getNetwork()).chainId.toString(),
      deployer: deployer.address,
      deploymentTime: new Date().toISOString(),
      nxsTokenAddress: NXS_TOKEN_ADDRESS,
      initialAdmin: INITIAL_ADMIN,
      verificationQuorum: VERIFICATION_QUORUM,
      transactionHash: deployTx ? deployTx.hash : null,
    };

    // Save to deployment.json
    const deploymentPath = path.join(__dirname, "..", "deployment.json");
    fs.writeFileSync(deploymentPath, JSON.stringify(deploymentInfo, null, 2));
    console.log("📄 Deployment info saved to:", deploymentPath);

    // Update Settings.toml with contract address
    await updateSettingsToml(contractAddress);

    console.log("\n🎉 Deployment completed successfully!");
    console.log("📋 Next steps:");
    console.log("  1. Verify the contract (optional): npm run verify", contractAddress);
    console.log("  2. Grant roles to mesh nodes using the admin functions");
    console.log("  3. Start the Rust validation engine: cargo run");
    console.log("  4. Test the integration with the deployed contract");

    return {
      contractAddress,
      deploymentInfo,
    };

  } catch (error) {
    console.error("❌ Deployment failed:", error.message);
    
    if (error.message.includes("insufficient funds")) {
      console.error("💸 Insufficient funds for deployment. Please add more RON to your wallet.");
    } else if (error.message.includes("nonce")) {
      console.error("🔄 Nonce error. Try again or reset your wallet nonce.");
    } else if (error.message.includes("gas")) {
      console.error("⛽ Gas estimation failed. Check network connectivity and contract code.");
    }
    
    throw error;
  }
}

async function updateSettingsToml(contractAddress) {
  try {
    const settingsPath = path.join(__dirname, "..", "Settings.toml");
    
    if (fs.existsSync(settingsPath)) {
      let content = fs.readFileSync(settingsPath, "utf8");
      
      // Update or add the contract address
      if (content.includes("# aura_protocol_address")) {
        content = content.replace(
          /# aura_protocol_address = ".*"/,
          `aura_protocol_address = "${contractAddress}"`
        );
      } else if (content.includes("aura_protocol_address")) {
        content = content.replace(
          /aura_protocol_address = ".*"/,
          `aura_protocol_address = "${contractAddress}"`
        );
      } else {
        // Add the contract address after the comment
        content = content.replace(
          /# Leave commented out to run without contract integration/,
          `# Leave commented out to run without contract integration\naura_protocol_address = "${contractAddress}"`
        );
      }
      
      fs.writeFileSync(settingsPath, content);
      console.log("📝 Updated Settings.toml with contract address");
    } else {
      console.warn("⚠️  Settings.toml not found. Please manually add the contract address.");
    }
  } catch (error) {
    console.warn("⚠️  Failed to update Settings.toml:", error.message);
    console.log("📝 Please manually add this line to Settings.toml:");
    console.log(`aura_protocol_address = "${contractAddress}"`);
  }
}

// Handle script execution
if (require.main === module) {
  main()
    .then(() => process.exit(0))
    .catch((error) => {
      console.error(error);
      process.exit(1);
    });
}

module.exports = { main };
