import subprocess
import sys

def main():
    service_name = "postgres"
    print(f"Checking if {service_name} dependency is running...")

    try:
        # Check if the container is running
        result = subprocess.run(
            ["docker", "compose", "ps", "--services", "--filter", "status=running"],
            capture_output=True,
            text=True,
            check=True
        )
        
        running_services = result.stdout.strip().split('\n')
        
        if service_name in running_services:
            print(f"Dependency '{service_name}' is already running.")
            return

        print(f"Dependency '{service_name}' is not running. Starting it now...")
        
        # Start the container
        start_result = subprocess.run(
            ["docker", "compose", "up", "-d", service_name],
            capture_output=True,
            text=True
        )
        
        if start_result.returncode != 0:
            print(f"Failed to start '{service_name}'.")
            print(f"Error details:\n{start_result.stderr}")
            sys.exit(1)
            
        print(f"Successfully started '{service_name}'.")

    except subprocess.CalledProcessError as e:
        print("Failed to check docker compose status. Is Docker running?")
        print(f"Error details:\n{e.stderr}")
        sys.exit(1)
    except FileNotFoundError:
        print("Docker is not installed or not found in PATH.")
        sys.exit(1)
    except Exception as e:
        print(f"An unexpected error occurred: {e}")
        sys.exit(1)

if __name__ == "__main__":
    main()
