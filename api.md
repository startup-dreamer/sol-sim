curl -X POST http://localhost:8080/forks \
-H "Content-Type: application/json" \
-d '{"accounts": ["11111111111111111111111111111111", "JUP6LkbZbjS1jKKwapdHNy74zcZ3tLUZoi5QNyVTaV4","GRQTRUPAvg9Fzcvwgp4ZmuCVwwhqfJAe5WmV6uKahwUk"]}'

curl -X POST http://127.0.0.1:8080/rpc/c543a66d-d261-4dc2-b400-150f3996d10f \
-H "Content-Type: application/json" \
-d '{"jsonrpc": "2.0", "id": 1, "method": "getBalance", "params": ["GRQTRUPAvg9Fzcvwgp4ZmuCVwwhqfJAe5WmV6uKahwUk"]}'

curl -X POST http://127.0.0.1:8080/rpc/c543a66d-d261-4dc2-b400-150f3996d10f \
-H "Content-Type: application/json" \
-d '{"jsonrpc": "2.0", "id": 1, "method": "getAccountInfo", "params": ["JUP6LkbZbjS1jKKwapdHNy74zcZ3tLUZoi5QNyVTaV4"]}'


curl -X POST http://127.0.0.1:8080/rpc/c543a66d-d261-4dc2-b400-150f3996d10f \
-H "Content-Type: application/json" \
-d '{"jsonrpc": "2.0", "id": 1, "method": "setAccount", "params": ["GRQTRUPAvg9Fzcvwgp4ZmuCVwwhqfJAe5WmV6uKahwUk", { "lamports": 5000000000, "data": "", "owner": "11111111111111111111111111111111", "executable": false }]}'

curl -X GET http://127.0.0.1:8080/rpc/c543a66d-d261-4dc2-b400-150f3996d10f

curl -X DELETE http://127.0.0.1:8080/forks/c543a66d-d261-4dc2-b400-150f3996d10f
