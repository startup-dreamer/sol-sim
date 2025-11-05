curl -X POST http://localhost:8080/forks \
-H "Content-Type: application/json" \
-d '{"accounts": ["11111111111111111111111111111111", "JUP6LkbZbjS1jKKwapdHNy74zcZ3tLUZoi5QNyVTaV4","GRQTRUPAvg9Fzcvwgp4ZmuCVwwhqfJAe5WmV6uKahwUk"]}'

curl -X POST http://127.0.0.1:8080/rpc/8be96ce4-9cbd-41cc-9449-f8abe1cee631 \
-H "Content-Type: application/json" \
-d '{"jsonrpc": "2.0", "id": 1, "method": "getBalance", "params": ["GRQTRUPAvg9Fzcvwgp4ZmuCVwwhqfJAe5WmV6uKahwUk"]}'

curl -X POST http://127.0.0.1:8080/rpc/8be96ce4-9cbd-41cc-9449-f8abe1cee631 \
-H "Content-Type: application/json" \
-d '{"jsonrpc": "2.0", "id": 1, "method": "getAccountInfo", "params": ["JUP6LkbZbjS1jKKwapdHNy74zcZ3tLUZoi5QNyVTaV4"]}'


curl -X POST http://127.0.0.1:8080/rpc/8be96ce4-9cbd-41cc-9449-f8abe1cee631 \
-H "Content-Type: application/json" \
-d '{"jsonrpc": "2.0", "id": 1, "method": "setAccount", "params": ["GRQTRUPAvg9Fzcvwgp4ZmuCVwwhqfJAe5WmV6uKahwUk", { "lamports": 5000000000, "data": "", "owner": "11111111111111111111111111111111", "executable": false }]}'

curl -X GET http://127.0.0.1:8080/forks/0ce8281f-702d-4cdf-b6c7-35d8f27340d7

curl -X DELETE http://127.0.0.1:8080/forks/0ce8281f-702d-4cdf-b6c7-35d8f27340d7
