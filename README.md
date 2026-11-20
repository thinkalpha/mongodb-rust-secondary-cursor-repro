# MongoDB Rust cursor cleanup routes to the wrong replica member

Run the repro with Docker Compose.

```sh
./repro.sh
```

The fixture starts MongoDB 9.0.4 with two members on an isolated Compose network and uses the Rust driver at 3.10.2. The program seeds six documents, waits for replication, and opens an aggregation cursor with read preference `secondary` and batch size 2. It advances three times to trigger `getMore`, then drops the cursor with rows still pending.

Observed command destinations:

```text
aggregate -> mongo2:27017
getMore -> mongo2:27017
killCursors -> mongo1:27017
```

The expected `killCursors` destination is `mongo2:27017`, the member that opened the cursor. See the [MongoDB server selection specification](https://github.com/mongodb/specifications/blob/master/source/server-selection/server-selection.md#scope-and-general-requirements).

Exit status 1 reproduces the routing mismatch. Exit status 0 indicates that cleanup went to the member that opened the cursor. Exit status 2 indicates a missing command event or an unexpected cursor destination.

Remove the fixture and its volumes:

```sh
docker compose down -v --remove-orphans
```
