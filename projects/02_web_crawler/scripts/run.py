import subprocess
import os
import sys

def main():
    print("Starting postgres...")
    # Run from the project root (where docker-compose.yml is)
    subprocess.run(["docker", "compose", "up", "-d", "postgres"], check=True)
    
    env = os.environ.copy()
    # Note: port 5433 is mapped to 5432 in docker-compose.yml
    env["DATABASE_URL"] = "postgres://postgres:password@localhost:5433/web_crawler"
    
    cmd = ["cargo", "run"]
    if len(sys.argv) > 1:
        cmd.append("--")
        cmd.extend(sys.argv[1:])
    
    print(f"Running: {' '.join(cmd)}")
    try:
        subprocess.run(cmd, env=env)
    except KeyboardInterrupt:
        print("\nShutting down...")
        sys.exit(0)

if __name__ == "__main__":
    main()
