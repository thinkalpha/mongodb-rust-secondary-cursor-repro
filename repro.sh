#!/bin/sh
set -eu

docker compose up -d --wait mongo1 mongo2
docker compose exec -T mongo1 mongosh --quiet <<'JS'
try {
  const status = rs.status();
  if (status.ok !== 1) throw new Error("replica set status not ready");
  print("Replica set rs0 already initialized");
} catch (error) {
  if (!String(error).includes("not yet initialized") && !String(error).includes("no replset config")) throw error;
  rs.initiate({_id: "rs0", members: [
    {_id: 0, host: "mongo1:27017", priority: 1},
    {_id: 1, host: "mongo2:27017", priority: 0}
  ]});
  print("Initialized replica set rs0");
}
const until = Date.now() + 15000;
while (Date.now() < until) {
  try {
    const s = rs.status();
    if (s.members.length === 2 && s.members.every(m => m.stateStr === "PRIMARY" || m.stateStr === "SECONDARY")) {
      print("Replica set members are ready"); quit(0);
    }
  } catch (_) {}
  sleep(100);
}
throw new Error("Timed out waiting for replica set election");
JS
docker compose run --rm repro
