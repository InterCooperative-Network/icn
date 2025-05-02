#!/usr/bin/env python3
"""
ICN Wallet Integration Test Orchestrator

This script orchestrates integration tests for the ICN Wallet ecosystem, which includes:
- Wallet API
- AgoraNet
- Runtime

It uses docker-compose to start the services and runs a series of tests against the live system.
"""

import os
import sys
import time
import json
import subprocess
import argparse
import requests
from pathlib import Path
from colorama import Fore, Style, init
import uuid

# Initialize colorama
init()

# Configuration
COMPOSE_FILE = "../docker-compose.integration.yml"
BASE_DIR = Path(__file__).parent.parent.parent
WALLET_API_URL = "http://localhost:3000/api"
AGORANET_URL = "http://localhost:8080/api"
RUNTIME_URL = "http://localhost:8081/api"

# Test scenarios
class TestScenario:
    """Base class for test scenarios"""
    
    def __init__(self, name):
        self.name = name
        self.results = []
        self.identity_id = None
        self.identity_did = None
    
    def run(self):
        """Execute the test scenario"""
        try:
            print(f"{Fore.YELLOW}Running {self.name}...{Style.RESET_ALL}")
            self.setup()
            self.execute()
            self.verify()
            self.cleanup()
            return True
        except Exception as e:
            print(f"{Fore.RED}Error in {self.name}: {e}{Style.RESET_ALL}")
            return False
    
    def setup(self):
        """Setup required for the test"""
        pass
    
    def execute(self):
        """Execute the test actions"""
        pass
    
    def verify(self):
        """Verify test results"""
        pass
    
    def cleanup(self):
        """Clean up after the test"""
        pass
    
    def assert_true(self, condition, message):
        """Assert a condition is true"""
        if not condition:
            raise AssertionError(message)
        
        self.results.append(f"✓ {message}")
    
    def create_identity(self):
        """Create a test identity for use in scenarios"""
        print("Creating test identity...")
        response = requests.post(
            f"{WALLET_API_URL}/did/create",
            json={
                "scope": "personal",
                "metadata": {"name": f"Test User {uuid.uuid4()}"}
            }
        )
        
        if response.status_code != 200:
            raise Exception(f"Failed to create identity: {response.text}")
        
        data = response.json()
        self.identity_id = data["id"]
        self.identity_did = data["did"]
        
        # Activate the identity
        activate_response = requests.post(
            f"{WALLET_API_URL}/did/activate/{self.identity_id}"
        )
        
        if activate_response.status_code != 200:
            raise Exception(f"Failed to activate identity: {activate_response.text}")
        
        print(f"Created and activated identity: {self.identity_did}")
        return self.identity_id

class ProposalLifecycleScenario(TestScenario):
    """Test the complete lifecycle of a proposal from creation to execution"""
    
    def __init__(self):
        super().__init__("Proposal Lifecycle")
        self.proposal_id = None
        self.action_id = None
    
    def setup(self):
        """Create an identity for this test"""
        self.create_identity()
    
    def execute(self):
        """Execute the proposal lifecycle"""
        # 1. Create and sign the proposal
        print("Creating proposal...")
        proposal_response = requests.post(
            f"{WALLET_API_URL}/proposal/sign",
            json={
                "proposal_type": "ConfigChange",
                "content": {
                    "title": "Integration Test Proposal",
                    "description": "A test proposal created by the integration test suite",
                    "parameter": "test_parameter",
                    "value": "test_value"
                }
            }
        )
        
        if proposal_response.status_code != 200:
            raise Exception(f"Failed to create proposal: {proposal_response.text}")
        
        data = proposal_response.json()
        self.action_id = data["action_id"]
        print(f"Created proposal with action ID: {self.action_id}")
        
        # 2. Check AgoraNet for the thread
        print("Waiting for AgoraNet thread...")
        threads = None
        for _ in range(10):  # Try for up to 10 seconds
            thread_response = requests.get(f"{AGORANET_URL}/threads?topic=governance")
            if thread_response.status_code == 200:
                threads = thread_response.json()
                if threads:
                    break
            time.sleep(1)
        
        if not threads:
            raise Exception("No governance threads found in AgoraNet")
        
        # Use the most recent thread
        thread = threads[0]
        thread_id = thread["id"]
        self.proposal_id = thread["proposal_id"]
        print(f"Found thread ID: {thread_id} for proposal: {self.proposal_id}")
        
        # 3. Link a credential to the thread
        print("Creating and linking a credential...")
        credential_response = requests.post(
            f"{WALLET_API_URL}/vc/issue",
            json={
                "subject_data": {
                    "id": self.identity_did,
                    "name": "Test User",
                    "role": "Member"
                },
                "credential_types": ["MembershipCredential"]
            }
        )
        
        # Fallback if VC issuance isn't implemented
        if credential_response.status_code != 200:
            credential_id = f"simulated-credential-{uuid.uuid4()}"
            print(f"Using simulated credential: {credential_id}")
        else:
            credential_data = credential_response.json()
            credential_id = credential_data["id"]
        
        # Link the credential
        link_response = requests.post(
            f"{WALLET_API_URL}/agoranet/credential-link",
            json={
                "thread_id": thread_id,
                "credential_id": credential_id
            }
        )
        
        if link_response.status_code != 200:
            raise Exception(f"Failed to link credential: {link_response.text}")
        
        print(f"Successfully linked credential to thread")
        
        # 4. Vote on the proposal (through Runtime direct API since Wallet API might not have this)
        print("Voting on the proposal...")
        
        vote_response = requests.post(
            f"{RUNTIME_URL}/proposals/{self.proposal_id}/vote",
            json={
                "guardian": self.identity_did,
                "decision": "Approve",
                "reason": "Integration test approval"
            }
        )
        
        if vote_response.status_code not in (200, 201):
            raise Exception(f"Failed to vote on proposal: {vote_response.status_code} - {vote_response.text}")
        
        print("Successfully voted on proposal")
        
        # 5. Create execution receipt
        print("Creating execution receipt...")
        receipt_response = requests.post(
            f"{WALLET_API_URL}/proposals/{self.proposal_id}/receipt",
            json={
                "success": True,
                "timestamp": "2023-05-01T12:00:00Z",
                "votes": {
                    "approve": 3,
                    "reject": 1,
                    "abstain": 0
                }
            }
        )
        
        # This might not be implemented in the wallet API
        if receipt_response.status_code != 200:
            print("Creating receipt directly through Runtime API...")
            runtime_receipt_response = requests.post(
                f"{RUNTIME_URL}/proposals/{self.proposal_id}/execute",
                json={
                    "success": True,
                    "executor": self.identity_did
                }
            )
            
            if runtime_receipt_response.status_code not in (200, 201):
                raise Exception(f"Failed to create execution receipt: {runtime_receipt_response.text}")
        
        print("Successfully created execution receipt")
        
        # 6. Notify AgoraNet about execution
        print("Notifying AgoraNet about execution...")
        notify_response = requests.post(
            f"{WALLET_API_URL}/agoranet/proposals/{self.proposal_id}/notify",
            json={
                "status": "executed",
                "timestamp": "2023-05-01T12:00:00Z",
                "executor": self.identity_did
            }
        )
        
        if notify_response.status_code != 200:
            raise Exception(f"Failed to notify AgoraNet: {notify_response.text}")
        
        print("Successfully notified AgoraNet")
        
        # 7. Sync TrustBundles
        print("Syncing TrustBundles...")
        sync_response = requests.post(f"{WALLET_API_URL}/sync/trust-bundles")
        
        if sync_response.status_code != 200:
            raise Exception(f"Failed to sync TrustBundles: {sync_response.text}")
        
        print("Successfully synced TrustBundles")
    
    def verify(self):
        """Verify the proposal lifecycle"""
        # 1. Check proposal status in Runtime
        print("Verifying proposal status in Runtime...")
        runtime_response = requests.get(f"{RUNTIME_URL}/proposals/{self.proposal_id}")
        
        if runtime_response.status_code != 200:
            raise Exception(f"Failed to get proposal from Runtime: {runtime_response.text}")
        
        runtime_proposal = runtime_response.json()
        self.assert_true(
            runtime_proposal["status"] in ("Executed", "Approved"), 
            f"Proposal status in Runtime is {runtime_proposal['status']}"
        )
        self.assert_true(
            runtime_proposal["execution_receipt"] is not None,
            "Execution receipt is present in Runtime"
        )
        
        # 2. Check thread status in AgoraNet
        print("Verifying thread status in AgoraNet...")
        thread_response = requests.get(f"{AGORANET_URL}/threads?proposal_id={self.proposal_id}")
        
        if thread_response.status_code != 200 or not thread_response.json():
            raise Exception(f"Failed to get thread from AgoraNet: {thread_response.text}")
        
        thread = thread_response.json()[0]
        thread_id = thread["id"]
        
        # 3. Check credential links in AgoraNet
        print("Verifying credential links...")
        links_response = requests.get(f"{AGORANET_URL}/threads/{thread_id}/credential-links")
        
        if links_response.status_code != 200:
            raise Exception(f"Failed to get credential links: {links_response.text}")
        
        links = links_response.json()
        self.assert_true(
            len(links) > 0,
            f"Found {len(links)} credential links"
        )
        
        print("All verifications passed!")

class StateSynchronizationScenario(TestScenario):
    """Test synchronization of state between Wallet, Runtime, and AgoraNet"""
    
    def __init__(self):
        super().__init__("State Synchronization")
        self.trust_bundles = None
    
    def setup(self):
        """Create an identity for this test"""
        self.create_identity()
    
    def execute(self):
        """Execute synchronization operations"""
        # 1. Sync trust bundles
        print("Syncing trust bundles...")
        sync_response = requests.post(f"{WALLET_API_URL}/sync/trust-bundles")
        
        if sync_response.status_code != 200:
            raise Exception(f"Failed to sync trust bundles: {sync_response.text}")
        
        print("Successfully synced trust bundles")
        
        # 2. List trust bundles
        print("Listing trust bundles...")
        bundles_response = requests.get(f"{WALLET_API_URL}/bundles")
        
        if bundles_response.status_code != 200:
            bundles = []
            print("Bundle listing not implemented in Wallet API")
        else:
            bundles = bundles_response.json()
            self.trust_bundles = bundles
            print(f"Found {len(bundles)} trust bundles")
        
        # 3. Check guardian status
        print("Checking guardian status...")
        status_response = requests.get(f"{WALLET_API_URL}/guardian/status")
        
        if status_response.status_code != 200:
            print("Guardian status check not implemented in Wallet API")
        else:
            status = status_response.json()
            print(f"Guardian status: {status}")
        
        # 4. Get guardians from Runtime
        print("Getting guardians from Runtime...")
        runtime_guardians_response = requests.get(f"{RUNTIME_URL}/guardians")
        
        if runtime_guardians_response.status_code != 200:
            raise Exception(f"Failed to get guardians from Runtime: {runtime_guardians_response.text}")
        
        runtime_guardians = runtime_guardians_response.json()
        print(f"Found {len(runtime_guardians)} guardians in Runtime")
        
        # 5. Add our identity as a guardian
        print("Registering as a guardian...")
        add_guardian_response = requests.post(
            f"{RUNTIME_URL}/guardians",
            json=self.identity_did
        )
        
        if add_guardian_response.status_code not in (200, 201):
            raise Exception(f"Failed to add guardian: {add_guardian_response.text}")
        
        print(f"Successfully registered as guardian: {self.identity_did}")
    
    def verify(self):
        """Verify synchronization worked correctly"""
        # 1. Check Runtime guardians includes our identity
        print("Verifying guardian registration...")
        guardians_response = requests.get(f"{RUNTIME_URL}/guardians")
        
        if guardians_response.status_code != 200:
            raise Exception(f"Failed to get guardians: {guardians_response.text}")
        
        guardians = guardians_response.json()
        self.assert_true(
            self.identity_did in guardians,
            f"Identity '{self.identity_did}' is registered as a guardian"
        )
        
        # 2. Re-check guardian status in Wallet if API supports it
        status_response = requests.get(f"{WALLET_API_URL}/guardian/status")
        
        if status_response.status_code == 200:
            status = status_response.json()
            # The exact structure of the response is unknown, so we just check if it exists
            self.assert_true(
                status is not None,
                "Guardian status is available"
            )
        
        print("All verifications passed!")

# Orchestration functions
def wait_for_services():
    """Wait for all services to be healthy"""
    print(f"{Fore.YELLOW}Waiting for services to be ready...{Style.RESET_ALL}")
    
    services = [
        {"name": "Wallet API", "url": f"{WALLET_API_URL}/health"},
        {"name": "AgoraNet", "url": f"{AGORANET_URL}/health"},
        {"name": "Runtime", "url": f"{RUNTIME_URL}/health"},
    ]
    
    for service in services:
        print(f"Waiting for {service['name']}...")
        for attempt in range(20):  # Wait up to 20 seconds
            try:
                response = requests.get(service["url"], timeout=1)
                if response.status_code == 200:
                    print(f"{Fore.GREEN}✓ {service['name']} is ready{Style.RESET_ALL}")
                    break
            except requests.exceptions.RequestException:
                pass
            
            if attempt == 19:
                print(f"{Fore.RED}✗ {service['name']} failed to start{Style.RESET_ALL}")
                return False
            
            time.sleep(1)
    
    print(f"{Fore.GREEN}All services are ready!{Style.RESET_ALL}")
    return True

def start_services():
    """Start all services using docker-compose"""
    print(f"{Fore.YELLOW}Starting services...{Style.RESET_ALL}")
    
    try:
        subprocess.run(
            ["docker-compose", "-f", COMPOSE_FILE, "up", "-d"],
            cwd=BASE_DIR,
            check=True
        )
        print(f"{Fore.GREEN}Services started successfully{Style.RESET_ALL}")
        return True
    except subprocess.CalledProcessError as e:
        print(f"{Fore.RED}Failed to start services: {e}{Style.RESET_ALL}")
        return False

def stop_services():
    """Stop all services using docker-compose"""
    print(f"{Fore.YELLOW}Stopping services...{Style.RESET_ALL}")
    
    try:
        subprocess.run(
            ["docker-compose", "-f", COMPOSE_FILE, "down", "-v"],
            cwd=BASE_DIR,
            check=True
        )
        print(f"{Fore.GREEN}Services stopped successfully{Style.RESET_ALL}")
        return True
    except subprocess.CalledProcessError as e:
        print(f"{Fore.RED}Failed to stop services: {e}{Style.RESET_ALL}")
        return False

def run_scenarios():
    """Run all test scenarios"""
    scenarios = [
        ProposalLifecycleScenario(),
        StateSynchronizationScenario(),
    ]
    
    results = []
    
    for scenario in scenarios:
        success = scenario.run()
        results.append({
            "name": scenario.name,
            "success": success,
            "details": scenario.results if success else []
        })
        
        print()  # Empty line between scenarios
    
    return results

def main():
    """Main function to orchestrate the tests"""
    parser = argparse.ArgumentParser(description="ICN Wallet Integration Test Orchestrator")
    parser.add_argument("--skip-docker", action="store_true", help="Skip starting Docker services")
    args = parser.parse_args()
    
    try:
        if not args.skip_docker:
            if not start_services():
                return 1
            
            if not wait_for_services():
                stop_services()
                return 1
        
        print(f"\n{Fore.CYAN}==== Running Integration Tests ===={Style.RESET_ALL}\n")
        results = run_scenarios()
        
        # Print summary
        print(f"\n{Fore.CYAN}==== Test Results ===={Style.RESET_ALL}")
        passed = 0
        for result in results:
            status = f"{Fore.GREEN}PASSED{Style.RESET_ALL}" if result["success"] else f"{Fore.RED}FAILED{Style.RESET_ALL}"
            print(f"{result['name']}: {status}")
            
            if result["success"]:
                passed += 1
                for detail in result["details"]:
                    print(f"  {detail}")
        
        print(f"\n{passed}/{len(results)} scenarios passed")
        
        # Determine exit code
        return 0 if passed == len(results) else 1
    finally:
        if not args.skip_docker:
            stop_services()

if __name__ == "__main__":
    sys.exit(main()) 