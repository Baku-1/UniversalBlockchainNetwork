#!/usr/bin/env python3
"""
Computation Test for Universal Blockchain Network
Tests the validation engine functionality and mesh network computations.
"""

import asyncio
import json
import time
import uuid
import hashlib
import websockets
from typing import Dict, List, Any, Optional
from dataclasses import dataclass, asdict
from enum import Enum
import logging

# Configure logging
logging.basicConfig(level=logging.INFO, format='%(asctime)s - %(levelname)s - %(message)s')
logger = logging.getLogger(__name__)

class TaskType(Enum):
    BLOCK_VALIDATION = "block_validation"
    GAME_STATE_UPDATE = "game_state_update"
    TRANSACTION_VALIDATION = "transaction_validation"
    CONFLICT_RESOLUTION = "conflict_resolution"

class TaskPriority(Enum):
    LOW = "low"
    NORMAL = "normal"
    HIGH = "high"
    CRITICAL = "critical"

@dataclass
class BlockToValidate:
    id: str
    data: bytes

@dataclass
class ValidatedBlock:
    id: str
    signature: bytes

@dataclass
class GameStateData:
    player_id: str
    action: str
    timestamp: float
    state_hash: bytes

@dataclass
class TransactionData:
    from_address: str
    to_address: str
    value: int
    gas_price: int
    nonce: int
    data: bytes

@dataclass
class ConflictData:
    conflicting_actions: List[GameStateData]
    resolution_strategy: str

@dataclass
class ComputationTask:
    id: str
    task_type: TaskType
    data: Dict[str, Any]
    priority: TaskPriority
    created_at: float

@dataclass
class TaskResult:
    task_id: str
    result: Dict[str, Any]
    processed_at: float
    processing_time_ms: int

class ValidationEngine:
    """Simulates the Rust validation engine functionality"""
    
    def __init__(self):
        self.stats = {
            "tasks_processed": 0,
            "tasks_failed": 0,
            "total_processing_time": 0,
            "start_time": time.time()
        }
    
    async def process_computation_task(self, task: ComputationTask) -> TaskResult:
        """Process a computation task and return the result"""
        start_time = time.time()
        
        try:
            logger.info(f"Processing task {task.id} of type {task.task_type.value}")
            
            if task.task_type == TaskType.BLOCK_VALIDATION:
                result = await self.validate_block(task.data)
            elif task.task_type == TaskType.GAME_STATE_UPDATE:
                result = await self.validate_game_state(task.data)
            elif task.task_type == TaskType.TRANSACTION_VALIDATION:
                result = await self.validate_transaction(task.data)
            elif task.task_type == TaskType.CONFLICT_RESOLUTION:
                result = await self.resolve_conflict(task.data)
            else:
                raise ValueError(f"Unknown task type: {task.task_type}")
            
            processing_time = int((time.time() - start_time) * 1000)
            self.stats["tasks_processed"] += 1
            self.stats["total_processing_time"] += processing_time
            
            return TaskResult(
                task_id=task.id,
                result=result,
                processed_at=time.time(),
                processing_time_ms=processing_time
            )
            
        except Exception as e:
            logger.error(f"Task {task.id} failed: {e}")
            self.stats["tasks_failed"] += 1
            return TaskResult(
                task_id=task.id,
                result={"error": str(e)},
                processed_at=time.time(),
                processing_time_ms=int((time.time() - start_time) * 1000)
            )
    
    async def validate_block(self, data: Dict[str, Any]) -> Dict[str, Any]:
        """Validate a blockchain block"""
        block_id = data.get("id", "")
        block_data = data.get("data", b"")
        
        # Simulate block validation logic
        await asyncio.sleep(0.1)  # Simulate processing time
        
        # Create a signature (simulated)
        signature = hashlib.sha256(block_data).digest()
        
        return {
            "type": "block_validated",
            "block_id": block_id,
            "valid": True,
            "signature": signature.hex(),
            "validation_time": time.time()
        }
    
    async def validate_game_state(self, data: Dict[str, Any]) -> Dict[str, Any]:
        """Validate a game state update"""
        player_id = data.get("player_id", "")
        action = data.get("action", "")
        state_hash = data.get("state_hash", b"")
        
        # Simulate game state validation
        await asyncio.sleep(0.05)
        
        # Generate new state hash
        new_state_hash = hashlib.sha256(f"{player_id}:{action}:{time.time()}".encode()).digest()
        
        return {
            "type": "game_state_validated",
            "player_id": player_id,
            "valid": True,
            "new_state_hash": new_state_hash.hex(),
            "action": action
        }
    
    async def validate_transaction(self, data: Dict[str, Any]) -> Dict[str, Any]:
        """Validate a transaction"""
        from_addr = data.get("from_address", "")
        to_addr = data.get("to_address", "")
        value = data.get("value", 0)
        
        # Simulate transaction validation
        await asyncio.sleep(0.08)
        
        # Generate transaction hash
        tx_hash = hashlib.sha256(f"{from_addr}:{to_addr}:{value}:{time.time()}".encode()).digest()
        
        return {
            "type": "transaction_validated",
            "valid": True,
            "transaction_hash": tx_hash.hex(),
            "gas_used": 21000,
            "from": from_addr,
            "to": to_addr,
            "value": value
        }
    
    async def resolve_conflict(self, data: Dict[str, Any]) -> Dict[str, Any]:
        """Resolve conflicts between mesh nodes"""
        conflicting_actions = data.get("conflicting_actions", [])
        resolution_strategy = data.get("resolution_strategy", "timestamp_based")
        
        # Simulate conflict resolution
        await asyncio.sleep(0.12)
        
        resolved_actions = []
        rejected_actions = []
        
        for action in conflicting_actions:
            # Simple resolution: accept actions with even timestamps
            if int(action.get("timestamp", 0)) % 2 == 0:
                resolved_actions.append(action)
            else:
                rejected_actions.append(action)
        
        return {
            "type": "conflict_resolved",
            "resolved": len(resolved_actions) > 0,
            "resolved_actions": len(resolved_actions),
            "rejected_actions": len(rejected_actions),
            "resolution_strategy": resolution_strategy
        }
    
    def get_stats(self) -> Dict[str, Any]:
        """Get engine statistics"""
        uptime = time.time() - self.stats["start_time"]
        avg_processing_time = (
            self.stats["total_processing_time"] / max(self.stats["tasks_processed"], 1)
        )
        
        return {
            "uptime_seconds": uptime,
            "tasks_processed": self.stats["tasks_processed"],
            "tasks_failed": self.stats["tasks_failed"],
            "success_rate": (
                self.stats["tasks_processed"] / 
                max(self.stats["tasks_processed"] + self.stats["tasks_failed"], 1)
            ),
            "average_processing_time_ms": avg_processing_time,
            "total_processing_time_ms": self.stats["total_processing_time"]
        }

class ComputationTest:
    """Main test class for computation engine testing"""
    
    def __init__(self, engine_port: int = 9898):
        self.engine_port = engine_port
        self.validation_engine = ValidationEngine()
        self.test_results = []
    
    async def connect_to_engine(self) -> Optional[websockets.WebSocketServerProtocol]:
        """Connect to the running Rust engine via WebSocket"""
        try:
            uri = f"ws://localhost:{self.engine_port}"
            logger.info(f"Attempting to connect to engine at {uri}")
            websocket = await websockets.connect(uri)
            logger.info("Successfully connected to engine")
            return websocket
        except Exception as e:
            logger.warning(f"Could not connect to engine: {e}")
            return None
    
    async def run_local_tests(self):
        """Run computation tests locally without engine connection"""
        logger.info("Starting local computation tests...")
        
        # Test 1: Block Validation
        await self.test_block_validation()
        
        # Test 2: Game State Validation
        await self.test_game_state_validation()
        
        # Test 3: Transaction Validation
        await self.test_transaction_validation()
        
        # Test 4: Conflict Resolution
        await self.test_conflict_resolution()
        
        # Test 5: Performance Test
        await self.test_performance()
        
        # Test 6: Error Handling
        await self.test_error_handling()
        
        logger.info("Local computation tests completed!")
        self.print_test_summary()
    
    async def test_block_validation(self):
        """Test block validation functionality"""
        logger.info("Testing block validation...")
        
        task = ComputationTask(
            id=str(uuid.uuid4()),
            task_type=TaskType.BLOCK_VALIDATION,
            data={
                "id": "block_123",
                "data": b"test_block_data_12345"
            },
            priority=TaskPriority.HIGH,
            created_at=time.time()
        )
        
        result = await self.validation_engine.process_computation_task(task)
        self.test_results.append(("Block Validation", result))
        
        assert result.result.get("valid") == True
        assert "signature" in result.result
        logger.info("✓ Block validation test passed")
    
    async def test_game_state_validation(self):
        """Test game state validation functionality"""
        logger.info("Testing game state validation...")
        
        task = ComputationTask(
            id=str(uuid.uuid4()),
            task_type=TaskType.GAME_STATE_UPDATE,
            data={
                "player_id": "player_456",
                "action": "move_forward",
                "state_hash": b"current_state_hash",
                "timestamp": time.time()
            },
            priority=TaskPriority.NORMAL,
            created_at=time.time()
        )
        
        result = await self.validation_engine.process_computation_task(task)
        self.test_results.append(("Game State Validation", result))
        
        assert result.result.get("valid") == True
        assert "new_state_hash" in result.result
        logger.info("✓ Game state validation test passed")
    
    async def test_transaction_validation(self):
        """Test transaction validation functionality"""
        logger.info("Testing transaction validation...")
        
        task = ComputationTask(
            id=str(uuid.uuid4()),
            task_type=TaskType.TRANSACTION_VALIDATION,
            data={
                "from_address": "0x1234567890abcdef",
                "to_address": "0xfedcba0987654321",
                "value": 1000000000000000000,  # 1 RON
                "gas_price": 20000000000,
                "nonce": 42,
                "data": b"transaction_data"
            },
            priority=TaskPriority.CRITICAL,
            created_at=time.time()
        )
        
        result = await self.validation_engine.process_computation_task(task)
        self.test_results.append(("Transaction Validation", result))
        
        assert result.result.get("valid") == True
        assert "transaction_hash" in result.result
        logger.info("✓ Transaction validation test passed")
    
    async def test_conflict_resolution(self):
        """Test conflict resolution functionality"""
        logger.info("Testing conflict resolution...")
        
        conflicting_actions = [
            {"player_id": "player1", "action": "move_left", "timestamp": 1000},
            {"player_id": "player2", "action": "move_right", "timestamp": 1001},
            {"player_id": "player3", "action": "jump", "timestamp": 1002}
        ]
        
        task = ComputationTask(
            id=str(uuid.uuid4()),
            task_type=TaskType.CONFLICT_RESOLUTION,
            data={
                "conflicting_actions": conflicting_actions,
                "resolution_strategy": "timestamp_based"
            },
            priority=TaskPriority.HIGH,
            created_at=time.time()
        )
        
        result = await self.validation_engine.process_computation_task(task)
        self.test_results.append(("Conflict Resolution", result))
        
        assert "resolved" in result.result
        assert "resolved_actions" in result.result
        logger.info("✓ Conflict resolution test passed")
    
    async def test_performance(self):
        """Test performance with multiple concurrent tasks"""
        logger.info("Testing performance with concurrent tasks...")
        
        tasks = []
        for i in range(10):
            task = ComputationTask(
                id=str(uuid.uuid4()),
                task_type=TaskType.BLOCK_VALIDATION,
                data={
                    "id": f"block_{i}",
                    "data": f"test_data_{i}".encode()
                },
                priority=TaskPriority.NORMAL,
                created_at=time.time()
            )
            tasks.append(task)
        
        start_time = time.time()
        results = await asyncio.gather(*[
            self.validation_engine.process_computation_task(task) 
            for task in tasks
        ])
        end_time = time.time()
        
        total_time = end_time - start_time
        avg_time = total_time / len(results)
        
        self.test_results.append(("Performance Test", {
            "total_tasks": len(tasks),
            "total_time": total_time,
            "avg_time_per_task": avg_time,
            "tasks_per_second": len(tasks) / total_time
        }))
        
        logger.info(f"✓ Performance test completed: {len(tasks)} tasks in {total_time:.2f}s")
    
    async def test_error_handling(self):
        """Test error handling with invalid tasks"""
        logger.info("Testing error handling...")
        
        # Test with invalid task type
        task = ComputationTask(
            id=str(uuid.uuid4()),
            task_type=TaskType.BLOCK_VALIDATION,  # Will be overridden
            data={"invalid": "data"},
            priority=TaskPriority.LOW,
            created_at=time.time()
        )
        
        # Manually set invalid task type
        task.task_type = None  # This will cause an error
        
        result = await self.validation_engine.process_computation_task(task)
        self.test_results.append(("Error Handling", result))
        
        assert "error" in result.result
        logger.info("✓ Error handling test passed")
    
    def print_test_summary(self):
        """Print a summary of all test results"""
        logger.info("\n" + "="*50)
        logger.info("COMPUTATION TEST SUMMARY")
        logger.info("="*50)
        
        for test_name, result in self.test_results:
            if isinstance(result, TaskResult):
                status = "✓ PASS" if "error" not in result.result else "✗ FAIL"
                processing_time = result.processing_time_ms
                logger.info(f"{test_name}: {status} ({processing_time}ms)")
            else:
                logger.info(f"{test_name}: {result}")
        
        # Print engine statistics
        stats = self.validation_engine.get_stats()
        logger.info("\nEngine Statistics:")
        logger.info(f"  Uptime: {stats['uptime_seconds']:.2f}s")
        logger.info(f"  Tasks Processed: {stats['tasks_processed']}")
        logger.info(f"  Tasks Failed: {stats['tasks_failed']}")
        logger.info(f"  Success Rate: {stats['success_rate']:.2%}")
        logger.info(f"  Avg Processing Time: {stats['average_processing_time_ms']:.2f}ms")
        
        logger.info("="*50)
    
    async def run_engine_integration_tests(self):
        """Run tests that integrate with the running Rust engine"""
        websocket = await self.connect_to_engine()
        if not websocket:
            logger.warning("Engine not available, skipping integration tests")
            return
        
        try:
            logger.info("Running engine integration tests...")
            
            # Test 1: Send computation task to engine
            task_data = {
                "type": "computation_task",
                "task": {
                    "id": str(uuid.uuid4()),
                    "task_type": "block_validation",
                    "data": {
                        "id": "integration_test_block",
                        "data": b"integration_test_data"
                    },
                    "priority": "high"
                }
            }
            
            await websocket.send(json.dumps(task_data))
            logger.info("✓ Sent computation task to engine")
            
            # Test 2: Listen for engine responses
            try:
                response = await asyncio.wait_for(websocket.recv(), timeout=5.0)
                response_data = json.loads(response)
                logger.info(f"✓ Received response from engine: {response_data}")
            except asyncio.TimeoutError:
                logger.warning("No response from engine within timeout")
            
        except Exception as e:
            logger.error(f"Integration test failed: {e}")
        finally:
            await websocket.close()

async def main():
    """Main test runner"""
    logger.info("Starting Universal Blockchain Network Computation Tests")
    logger.info("="*60)
    
    test = ComputationTest()
    
    # Run local tests first
    await test.run_local_tests()
    
    # Try to run integration tests with engine
    await test.run_engine_integration_tests()
    
    logger.info("All tests completed!")

if __name__ == "__main__":
    asyncio.run(main()) 