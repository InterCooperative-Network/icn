#!/usr/bin/env python3
"""
ICN Configuration Validator

Validates ICN configuration files against the JSON schema.
Supports both TOML and JSON configuration files.

Usage:
    python3 scripts/validate-config.py config/icn.toml
    python3 scripts/validate-config.py --format json config/icn.json
"""

import json
import sys
import argparse
from pathlib import Path

try:
    import jsonschema
    from jsonschema import validate, ValidationError
except ImportError:
    print("Error: jsonschema package not installed")
    print("Install with: pip install jsonschema")
    sys.exit(1)

try:
    import toml
except ImportError:
    print("Error: toml package not installed")
    print("Install with: pip install toml")
    sys.exit(1)


def load_schema(schema_path):
    """Load JSON schema from file."""
    with open(schema_path, 'r') as f:
        return json.load(f)


def load_config_toml(config_path):
    """Load TOML configuration and convert to dict."""
    with open(config_path, 'r') as f:
        return toml.load(f)


def load_config_json(config_path):
    """Load JSON configuration."""
    with open(config_path, 'r') as f:
        return json.load(f)


def validate_config(config, schema):
    """Validate configuration against schema."""
    try:
        validate(instance=config, schema=schema)
        return True, []
    except ValidationError as e:
        return False, [str(e)]


def print_validation_errors(errors):
    """Print validation errors in a readable format."""
    print("\n❌ Configuration Validation Failed:")
    print("=" * 70)
    for i, error in enumerate(errors, 1):
        print(f"\nError {i}:")
        print(error)
    print("=" * 70)


def print_success(config_path):
    """Print success message."""
    print(f"\n✅ Configuration Valid: {config_path}")
    print("All settings comply with the ICN configuration schema.")


def main():
    parser = argparse.ArgumentParser(
        description='Validate ICN configuration files',
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog='''
Examples:
  # Validate TOML configuration
  %(prog)s config/icn.toml
  
  # Validate JSON configuration
  %(prog)s --format json config/icn.json
  
  # Use custom schema
  %(prog)s --schema my-schema.json config/icn.toml
        '''
    )
    parser.add_argument(
        'config_file',
        help='Path to configuration file'
    )
    parser.add_argument(
        '--format',
        choices=['toml', 'json'],
        default='toml',
        help='Configuration file format (default: toml)'
    )
    parser.add_argument(
        '--schema',
        default='config/icn-config.schema.json',
        help='Path to JSON schema file'
    )
    parser.add_argument(
        '--verbose',
        action='store_true',
        help='Show detailed validation information'
    )
    
    args = parser.parse_args()
    
    # Check if files exist
    config_path = Path(args.config_file)
    schema_path = Path(args.schema)
    
    if not config_path.exists():
        print(f"Error: Configuration file not found: {config_path}")
        sys.exit(1)
    
    if not schema_path.exists():
        print(f"Error: Schema file not found: {schema_path}")
        sys.exit(1)
    
    # Load schema
    if args.verbose:
        print(f"Loading schema: {schema_path}")
    
    try:
        schema = load_schema(schema_path)
    except Exception as e:
        print(f"Error loading schema: {e}")
        sys.exit(1)
    
    # Load configuration
    if args.verbose:
        print(f"Loading configuration: {config_path}")
    
    try:
        if args.format == 'toml':
            config = load_config_toml(config_path)
        else:
            config = load_config_json(config_path)
    except Exception as e:
        print(f"Error loading configuration: {e}")
        sys.exit(1)
    
    if args.verbose:
        print(f"Configuration loaded successfully")
        print(f"Top-level keys: {list(config.keys())}")
    
    # Validate
    if args.verbose:
        print("\nValidating configuration...")
    
    is_valid, errors = validate_config(config, schema)
    
    if is_valid:
        print_success(config_path)
        
        if args.verbose:
            print("\nConfiguration Summary:")
            if 'network' in config:
                print(f"  Network: {config['network'].get('listen_addr', 'default')}")
            if 'observability' in config:
                print(f"  Log Level: {config['observability'].get('log_level', 'info')}")
            if 'gateway' in config and config['gateway'].get('enabled'):
                print(f"  Gateway: Enabled at {config['gateway'].get('bind_addr', 'default')}")
        
        return 0
    else:
        print_validation_errors(errors)
        return 1


if __name__ == '__main__':
    sys.exit(main())
