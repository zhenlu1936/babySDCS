docker rm -f server1 server2 server3 2>/dev/null || true

docker build -t babysdcs:latest .

docker network create baby-net 2>/dev/null || true

docker create --name server1 --network baby-net -p 9527:8001 \
  -e PEERS="server1:8001,server2:8002,server3:8003" \
  -e PORT=8001 -e NAME=server1 babysdcs:latest

docker create --name server2 --network baby-net -p 9528:8002 \
  -e PEERS="server1:8001,server2:8002,server3:8003" \
  -e PORT=8002 -e NAME=server2 babysdcs:latest

docker create --name server3 --network baby-net -p 9529:8003 \
  -e PEERS="server1:8001,server2:8002,server3:8003" \
  -e PORT=8003 -e NAME=server3 babysdcs:latest