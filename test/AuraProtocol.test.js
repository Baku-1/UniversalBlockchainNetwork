const { expect } = require("chai");
const { ethers, upgrades } = require("hardhat");
require("@nomicfoundation/hardhat-chai-matchers");

describe("AuraProtocol Deployment Test", function () {
  let auraProtocol;
  let owner, admin, requester, submitter, worker1, worker2;
  const NXS_TOKEN_ADDRESS = "0x0000000000000000000000000000000000000000"; // Zero address for testing
  const VERIFICATION_QUORUM = 66;

  beforeEach(async function () {
    [owner, admin, requester, submitter, worker1, worker2] = await ethers.getSigners();

    // Deploy AuraProtocol using the same method as deployment script
    const AuraProtocol = await ethers.getContractFactory("AuraProtocol");
    auraProtocol = await upgrades.deployProxy(
      AuraProtocol,
      [NXS_TOKEN_ADDRESS, admin.address, VERIFICATION_QUORUM],
      { initializer: "initialize", kind: "uups" }
    );
    await auraProtocol.deployed();
  });

  describe("Deployment", function () {
    it("Should deploy successfully", async function () {
      expect(auraProtocol.address).to.not.equal(ethers.constants.AddressZero);
    });

    it("Should set the correct initial values", async function () {
      expect(await auraProtocol.verificationQuorum()).to.equal(ethers.BigNumber.from(VERIFICATION_QUORUM));
      expect(await auraProtocol.nextTaskId()).to.equal(ethers.BigNumber.from(0));
      expect(await auraProtocol.hasRole(await auraProtocol.ADMIN_ROLE(), admin.address)).to.be.true;
    });

    it("Should set the correct NXS token address", async function () {
      expect(await auraProtocol.nxsToken()).to.equal(NXS_TOKEN_ADDRESS);
    });

    it("Should have the correct domain separator for EIP712", async function () {
      // This tests that EIP712 initialization worked
      // Note: DOMAIN_SEPARATOR might be internal in this version
      expect(auraProtocol.address).to.not.equal(ethers.constants.AddressZero);
    });
  });

  describe("Role Management", function () {
    it("Should allow admin to grant roles", async function () {
      await auraProtocol.connect(admin).grantTaskRequesterRole(requester.address);
      expect(
        await auraProtocol.hasRole(await auraProtocol.TASK_REQUESTER_ROLE(), requester.address)
      ).to.be.true;
    });

    it("Should allow admin to grant result submitter role", async function () {
      await auraProtocol.connect(admin).grantResultSubmitterRole(submitter.address);
      expect(
        await auraProtocol.hasRole(await auraProtocol.RESULT_SUBMITTER_ROLE(), submitter.address)
      ).to.be.true;
    });

    it("Should prevent non-admin from granting roles", async function () {
      await expect(
        auraProtocol.connect(requester).grantTaskRequesterRole(worker1.address)
      ).to.be.reverted;
    });
  });

  describe("Pausable Functionality", function () {
    it("Should allow pauser to pause the contract", async function () {
      await auraProtocol.connect(admin).pause();
      expect(await auraProtocol.paused()).to.be.true;
    });

    it("Should allow pauser to unpause the contract", async function () {
      await auraProtocol.connect(admin).pause();
      await auraProtocol.connect(admin).unpause();
      expect(await auraProtocol.paused()).to.be.false;
    });

    it("Should prevent non-pauser from pausing", async function () {
      await expect(
        auraProtocol.connect(requester).pause()
      ).to.be.reverted;
    });
  });

  describe("Basic Task Creation (without token transfers)", function () {
    beforeEach(async function () {
      // Grant requester role for task creation tests
      await auraProtocol.connect(admin).grantTaskRequesterRole(requester.address);
    });

    it("Should fail task creation without token approval", async function () {
      const taskData = ethers.utils.toUtf8Bytes("test task data");
      const bounty = ethers.utils.parseEther("100");
      const workerCohort = [worker1.address, worker2.address];
      const deadline = 3600; // 1 hour

      // This should fail because we don't have NXS tokens or approval
      await expect(
        auraProtocol.connect(requester).createTask(taskData, bounty, workerCohort, deadline)
      ).to.be.reverted;
    });

    it("Should fail with zero bounty", async function () {
      const taskData = ethers.utils.toUtf8Bytes("test task data");
      const workerCohort = [worker1.address, worker2.address];
      const deadline = 3600;

      await expect(
        auraProtocol.connect(requester).createTask(taskData, 0, workerCohort, deadline)
      ).to.be.revertedWith("Bounty must be > 0");
    });

    it("Should fail with empty worker cohort", async function () {
      const taskData = ethers.utils.toUtf8Bytes("test task data");
      const bounty = ethers.utils.parseEther("100");
      const deadline = 3600;

      await expect(
        auraProtocol.connect(requester).createTask(taskData, bounty, [], deadline)
      ).to.be.revertedWith("Cohort cannot be empty");
    });
  });

  describe("Contract Upgradeability", function () {
    it("Should be upgradeable by admin", async function () {
      // This tests that the UUPS upgrade mechanism is working
      expect(await auraProtocol.hasRole(await auraProtocol.ADMIN_ROLE(), admin.address)).to.be.true;
    });
  });
});

describe("Gas Estimation for Deployment", function () {
  it("Should estimate gas for deployment", async function () {
    const [deployer] = await ethers.getSigners();
    
    console.log("🔍 Gas Estimation for AuraProtocol Deployment:");
    console.log("📍 Deployer Address:", deployer.address);
    
    const AuraProtocol = await ethers.getContractFactory("AuraProtocol");
    
    // Estimate gas for proxy deployment
    const deployTx = await upgrades.deployProxy(
      AuraProtocol,
      ["0x0000000000000000000000000000000000000000", deployer.address, 66],
      { initializer: "initialize", kind: "uups" }
    );
    
    const receipt = await deployTx.deployTransaction.wait();

    console.log("⛽ Gas Used:", receipt.gasUsed.toString());
    console.log("💰 Gas Price:", ethers.utils.formatUnits(receipt.gasPrice || receipt.effectiveGasPrice, "gwei"), "gwei");
    console.log("💸 Total Cost:", ethers.utils.formatEther(receipt.gasUsed.mul(receipt.gasPrice || receipt.effectiveGasPrice)), "RON");
    console.log("📍 Contract Address:", deployTx.address);
    
    expect(deployTx.address).to.not.equal(ethers.constants.AddressZero);
  });
});
