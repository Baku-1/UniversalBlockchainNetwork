require("@nomiclabs/hardhat-ethers");
require("@openzeppelin/hardhat-upgrades");
require("dotenv").config();

/** @type import('hardhat/config').HardhatUserConfig */
module.exports = {
  solidity: {
    version: "0.8.20",
    settings: {
      optimizer: {
        enabled: true,
        runs: 200,
      },
    },
  },
  networks: {
    hardhat: {
      chainId: 31337,
    },
    "ronin-testnet": {
      url: "https://saigon-testnet.roninchain.com/rpc",
      chainId: 2021,
      accounts: process.env.PRIVATE_KEY && process.env.PRIVATE_KEY !== "your_private_key_here" ? [`0x${process.env.PRIVATE_KEY}`] : [],
      gasPrice: 20000000000, // 20 gwei
      gas: 8000000,
    },
    "ronin-mainnet": {
      url: "https://api.roninchain.com/rpc",
      chainId: 2020,
      accounts: process.env.PRIVATE_KEY && process.env.PRIVATE_KEY !== "your_private_key_here" ? [`0x${process.env.PRIVATE_KEY}`] : [],
      gasPrice: 20000000000, // 20 gwei
      gas: 8000000,
    },
  },
  etherscan: {
    apiKey: {
      "ronin-testnet": process.env.RONIN_API_KEY || "dummy",
      "ronin-mainnet": process.env.RONIN_API_KEY || "dummy",
    },
    customChains: [
      {
        network: "ronin-testnet",
        chainId: 2021,
        urls: {
          apiURL: "https://saigon-app.roninchain.com/api",
          browserURL: "https://saigon-app.roninchain.com",
        },
      },
      {
        network: "ronin-mainnet",
        chainId: 2020,
        urls: {
          apiURL: "https://app.roninchain.com/api",
          browserURL: "https://app.roninchain.com",
        },
      },
    ],
  },
  paths: {
    sources: "./src/contracts",
    tests: "./test",
    cache: "./cache",
    artifacts: "./artifacts",
  },
  mocha: {
    timeout: 40000,
  },
};
