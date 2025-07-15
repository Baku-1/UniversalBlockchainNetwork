// SPDX-License-Identifier: MIT
pragma solidity ^0.8.20;

import "@openzeppelin/contracts-upgradeable/proxy/utils/Initializable.sol";
import "@openzeppelin/contracts-upgradeable/proxy/utils/UUPSUpgradeable.sol";
import "@openzeppelin/contracts-upgradeable/access/AccessControlUpgradeable.sol";
import "@openzeppelin/contracts-upgradeable/token/ERC20/IERC20Upgradeable.sol";
import "@openzeppelin/contracts-upgradeable/security/PausableUpgradeable.sol";
import "@openzeppelin/contracts-upgradeable/security/ReentrancyGuardUpgradeable.sol";
import "@openzeppelin/contracts-upgradeable/utils/cryptography/EIP712Upgradeable.sol";
import "@openzeppelin/contracts-upgradeable/utils/cryptography/ECDSAUpgradeable.sol";

/**
 * @title AuraProtocol (v3 - Hardened)
 * @author NexusLabs
 * @notice This contract is the on-chain brain for the AURA off-chain computation network.
 * It manages the entire lifecycle of computational tasks and has been hardened based on security audits.
 */
contract AuraProtocol is Initializable, UUPSUpgradeable, AccessControlUpgradeable, PausableUpgradeable, ReentrancyGuardUpgradeable, EIP712Upgradeable {

    // --- Roles ---
    bytes32 public constant TASK_REQUESTER_ROLE = keccak256("TASK_REQUESTER_ROLE");
    bytes32 public constant RESULT_SUBMITTER_ROLE = keccak256("RESULT_SUBMITTER_ROLE");
    bytes32 public constant PAUSER_ROLE = keccak256("PAUSER_ROLE");
    bytes32 public constant ADMIN_ROLE = keccak256("ADMIN_ROLE");

    // --- Enums ---
    enum TaskStatus {
        Open,
        Processing,
        Verifying,
        Completed,
        Failed,
        Cancelled
    }

    // --- Structs ---
    struct Task {
        uint256 id;
        address requester;
        bytes taskData;
        uint256 bounty;
        uint256 createdAt;
        uint256 submissionDeadline;
        TaskStatus status;
        address[] workerCohort;
        bytes32 resultHash; // Recommendation #10: Store only the hash of the result
    }

    // --- State Variables ---
    IERC20Upgradeable public nxsToken;
    uint256 public nextTaskId;
    uint256 public verificationQuorum;

    mapping(uint256 => Task) public tasks;

    // --- Events ---
    event TaskCreated(uint256 indexed taskId, address indexed requester, uint256 bounty, bytes taskData, uint256 deadline);
    event TaskResultSubmitted(uint256 indexed taskId, bytes result, bytes32 resultHash, address submitter); // Recommendation #9: Added event
    event TaskCompleted(uint256 indexed taskId);
    event TaskFailed(uint256 indexed taskId);
    event TaskCancelled(uint256 indexed taskId, string reason);

    /// @custom:oz-upgrades-unsafe-allow constructor
    constructor() {
        _disableInitializers();
    }

    function initialize(address _nxsTokenAddress, address _initialAdmin, uint256 _quorum) public initializer {
        __UUPSUpgradeable_init();
        __AccessControl_init();
        __Pausable_init();
        __ReentrancyGuard_init();
        __EIP712_init("AuraProtocol", "1");

        require(_quorum > 0 && _quorum <= 100, "Quorum must be between 1 and 100");
        nxsToken = IERC20Upgradeable(_nxsTokenAddress);
        verificationQuorum = _quorum;

        _grantRole(DEFAULT_ADMIN_ROLE, _initialAdmin);
        _grantRole(ADMIN_ROLE, _initialAdmin);
        _grantRole(PAUSER_ROLE, _initialAdmin);
    }

    // --- Core Functions ---

    function createTask(bytes calldata _taskData, uint256 _bounty, address[] calldata _workerCohort, uint32 _deadlineDuration)
        external
        whenNotPaused
        onlyRole(TASK_REQUESTER_ROLE)
        nonReentrant
    {
        require(_bounty > 0, "Bounty must be > 0");
        require(_workerCohort.length > 0, "Cohort cannot be empty");
        require(_deadlineDuration > 0, "Deadline must be in the future");

        nxsToken.transferFrom(msg.sender, address(this), _bounty);

        uint256 taskId = nextTaskId;
        uint256 deadline = block.timestamp + _deadlineDuration;
        tasks[taskId] = Task({
            id: taskId,
            requester: msg.sender,
            taskData: _taskData,
            bounty: _bounty,
            createdAt: block.timestamp,
            submissionDeadline: deadline,
            status: TaskStatus.Open,
            workerCohort: _workerCohort,
            resultHash: bytes32(0)
        });

        nextTaskId++;
        emit TaskCreated(taskId, msg.sender, _bounty, _taskData, deadline);
    }

    function submitResult(uint256 _taskId, bytes calldata _result, bytes[] calldata _signatures)
        external
        whenNotPaused
        onlyRole(RESULT_SUBMITTER_ROLE)
        nonReentrant
    {
        Task storage task = tasks[_taskId];
        require(task.status == TaskStatus.Open || task.status == TaskStatus.Processing, "Task not open for submission");
        require(block.timestamp <= task.submissionDeadline, "Submission deadline has passed");

        bool isVerified = _verifyCohortSignatures(_taskId, _result, _signatures);

        if (isVerified) {
            task.status = TaskStatus.Completed;
            task.resultHash = keccak256(_result); // Recommendation #10: Store hash
            nxsToken.transfer(msg.sender, task.bounty);
            emit TaskResultSubmitted(_taskId, _result, task.resultHash, msg.sender);
            emit TaskCompleted(_taskId);
        } else {
            task.status = TaskStatus.Failed;
            nxsToken.transfer(task.requester, task.bounty);
            emit TaskFailed(_taskId);
        }
    }

    // --- Internal & View Functions ---

    /**
     * @dev Verifies signatures from a worker cohort against a result hash.
     * Recommendation #7: Added dev comment on quorum calculation.
     * e.g., For a 3-person cohort and 66% quorum, 2 valid signatures are required. (2 * 100 >= 3 * 66) -> (200 >= 198)
     */
    function _verifyCohortSignatures(uint256 _taskId, bytes calldata _result, bytes[] calldata _signatures)
        internal view returns (bool)
    {
        Task storage task = tasks[_taskId];
        bytes32 digest = _hashTypedDataV4(keccak256(abi.encode(
            keccak256("TaskResult(uint256 taskId,bytes result)"),
            _taskId,
            keccak256(_result)
        )));

        uint256 validSignatures = 0;
        // CRITICAL FIX #1: Using a memory array to prevent duplicate signers, as local mappings are not allowed.
        address[] memory usedSigners = new address[](_signatures.length);
        uint256 signerCount = 0;

        for (uint i = 0; i < _signatures.length; i++) {
            address recoveredSigner = ECDSAUpgradeable.recover(digest, _signatures[i]);
            
            if (recoveredSigner != address(0) && _isSignerInCohort(recoveredSigner, task.workerCohort) && !_isDuplicateSigner(recoveredSigner, usedSigners, signerCount)) {
                usedSigners[signerCount] = recoveredSigner;
                signerCount++;
                validSignatures++;
            }
        }
        
        return (validSignatures * 100) >= (task.workerCohort.length * verificationQuorum);
    }

    /**
     * @dev Helper function to check for duplicate signers in a memory array.
     */
    function _isDuplicateSigner(address signer, address[] memory signers, uint count) internal pure returns (bool) {
        for (uint j = 0; j < count; j++) {
            if (signers[j] == signer) return true;
        }
        return false;
    }
    
    /**
     * @dev Checks if a signer is in the cohort.
     * Recommendation #8: Acknowledging gas cost of linear search for large cohorts.
     * A mapping in the Task struct (`mapping(address => bool) isWorker`) would be more gas-efficient for O(1) lookups.
     */
    function _isSignerInCohort(address _signer, address[] storage _cohort) internal view returns (bool) {
        for (uint i = 0; i < _cohort.length; i++) {
            if (_cohort[i] == _signer) {
                return true;
            }
        }
        return false;
    }

    // --- Admin & Pausable Functions ---

    function cancelStuckTask(uint256 _taskId) external onlyRole(ADMIN_ROLE) nonReentrant {
        Task storage task = tasks[_taskId];
        require(task.status == TaskStatus.Open || task.status == TaskStatus.Processing, "Task not in a cancellable state");
        require(block.timestamp > task.submissionDeadline, "Submission deadline has not passed");

        task.status = TaskStatus.Cancelled;
        nxsToken.transfer(task.requester, task.bounty);
        emit TaskCancelled(_taskId, "Cancelled by admin due to deadline.");
    }

    /**
     * @notice Allows the original task requester to cancel their own task, but ONLY if it's still open.
     */
    function requesterCancelTask(uint256 _taskId) external nonReentrant {
        Task storage task = tasks[_taskId];
        require(msg.sender == task.requester, "Only the task requester can cancel");
        require(task.status == TaskStatus.Open, "Cannot cancel a task that is already being processed");

        task.status = TaskStatus.Cancelled;
        nxsToken.transfer(task.requester, task.bounty);
        emit TaskCancelled(_taskId, "Cancelled by requester.");
    }

    function pause() public onlyRole(PAUSER_ROLE) {
        _pause();
    }

    function unpause() public onlyRole(PAUSER_ROLE) {
        _unpause();
    }

    // Recommendation #11: Public functions for role management by admin.
    function grantAdminRole(address account) public virtual onlyRole(ADMIN_ROLE) {
        grantRole(ADMIN_ROLE, account);
    }

    function grantPauserRole(address account) public virtual onlyRole(ADMIN_ROLE) {
        grantRole(PAUSER_ROLE, account);
    }

    function grantTaskRequesterRole(address account) public virtual onlyRole(ADMIN_ROLE) {
        grantRole(TASK_REQUESTER_ROLE, account);
    }

    function grantResultSubmitterRole(address account) public virtual onlyRole(ADMIN_ROLE) {
        grantRole(RESULT_SUBMITTER_ROLE, account);
    }

    function _authorizeUpgrade(address newImplementation) internal override onlyRole(ADMIN_ROLE) {}

    // Recommendation #12: Storage gap for future upgradeability.
    uint256[50] private __gap;
}
