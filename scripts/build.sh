docker rm -f server1 server2 server3 2>/dev/null || true

docker build -t babysdcs:latest .

docker network create baby-net 2>/dev/null || true