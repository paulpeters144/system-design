import subprocess
import os
import sys

def main():
    print("Starting postgres and redis...")
    # Run from the project root (where docker-compose.yml is)
    # We assume the script is called from justfile which is in the same dir as docker-compose.yml
    subprocess.run(["docker", "compose", "up", "-d", "postgres", "redis"], check=True)
    
    env = os.environ.copy()
    env["DATABASE_URL"] = "postgres://postgres:password@localhost:5432/system_design"
    env["REDIS_URL"] = "redis://localhost:6379/"
    
    print("Running application...")
    try:
        # We don't use check=True here so that we can handle the exit code if needed
        # but KeyboardInterrupt is the main thing we want to catch.
        subprocess.run(["cargo", "run"], env=env)
    except KeyboardInterrupt:
        print("\nShutting down...")
        sys.exit(0)

if __name__ == "__main__":
    main()
