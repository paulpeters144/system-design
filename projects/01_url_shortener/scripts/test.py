import subprocess
import os
import sys
import time

def check_postgres_ready(container_id):
    """Wait for postgres to be ready for connections."""
    result = subprocess.run(
        ["docker", "exec", container_id, "pg_isready", "-U", "postgres"],
        capture_output=True
    )
    return result.returncode == 0

def main():
    print("Starting postgres and redis for testing...")
    # docker-compose.yml is expected in the current working directory (project root)
    subprocess.run(["docker", "compose", "up", "-d", "postgres", "redis"], check=True)
    
    print("Waiting for postgres to be ready...")
    container_id_result = subprocess.run(
        ["docker", "compose", "ps", "-q", "postgres"],
        capture_output=True,
        text=True,
        check=True
    )
    container_id = container_id_result.stdout.strip()
    
    if not container_id:
        print("Error: Postgres container not found")
        sys.exit(1)
        
    ready = False
    for i in range(30):
        if check_postgres_ready(container_id):
            ready = True
            break
        if i % 5 == 0:
            print(f"Still waiting ({i+1}/30)...")
        time.sleep(1)
        
    if not ready:
        print("Error: Postgres failed to become ready")
        sys.exit(1)
        
    print("Checking for test database...")
    db_exists_cmd = [
        "docker", "exec", container_id, 
        "psql", "-U", "postgres", "-d", "postgres", "-tAc", 
        "SELECT 1 FROM pg_database WHERE datname='system_design_test'"
    ]
    db_exists_result = subprocess.run(db_exists_cmd, capture_output=True, text=True)
    db_exists = db_exists_result.stdout.strip()
    
    if db_exists != "1":
        print("Creating test database...")
        create_db_cmd = [
            "docker", "exec", container_id,
            "psql", "-U", "postgres", "-d", "postgres", "-c",
            "CREATE DATABASE system_design_test"
        ]
        subprocess.run(create_db_cmd, check=True)
    else:
        print("Test database already exists.")
        
    env = os.environ.copy()
    env["DATABASE_URL"] = "postgres://postgres:password@localhost:5432/system_design_test"
    env["REDIS_URL"] = "redis://localhost:6379/"
    
    print("Running tests with nextest...")
    try:
        subprocess.run(["cargo", "nextest", "run"], env=env, check=True)
    except subprocess.CalledProcessError as e:
        print(f"Tests failed with exit code: {e.returncode}")
        sys.exit(e.returncode)

if __name__ == "__main__":
    main()
