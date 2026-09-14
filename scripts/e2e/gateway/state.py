from __future__ import annotations

import contextlib
import datetime as dt
import re
import sqlite3
import threading
from pathlib import Path


class StateError(RuntimeError):
    pass


def parse_time(value: str) -> dt.datetime:
    parsed = dt.datetime.fromisoformat(value.replace("Z", "+00:00"))
    if parsed.tzinfo is None:
        raise ValueError("timestamp must include a timezone")
    return parsed.astimezone(dt.timezone.utc)


def format_time(value: dt.datetime) -> str:
    return value.astimezone(dt.timezone.utc).isoformat().replace("+00:00", "Z")


class LeaseStore:
    FIELDS = ("lease_id", "namespace", "provider", "owner", "run_id", "run_attempt", "expires_at", "heartbeat_at")

    def __init__(self, path: Path, provider: str, clock=None):
        self.path, self.provider = Path(path), provider
        self.clock = clock or (lambda: dt.datetime.now(dt.timezone.utc))
        self.lock = threading.RLock()
        self.path.parent.mkdir(parents=True, exist_ok=True)
        with self._db() as db:
            db.execute("PRAGMA journal_mode=WAL")
            db.execute("""CREATE TABLE IF NOT EXISTS leases (
              lease_id TEXT PRIMARY KEY, namespace TEXT NOT NULL UNIQUE, provider TEXT NOT NULL,
              owner TEXT NOT NULL, run_id TEXT NOT NULL, run_attempt TEXT NOT NULL,
              tested_sha TEXT NOT NULL, expires_at TEXT NOT NULL, heartbeat_at TEXT NOT NULL,
              ttl_seconds INTEGER NOT NULL, heartbeat_seconds INTEGER NOT NULL,
              max_concurrency INTEGER NOT NULL, qps INTEGER NOT NULL, status TEXT NOT NULL
            )""")

    @contextlib.contextmanager
    def _db(self):
        db = sqlite3.connect(self.path, timeout=5)
        try:
            yield db
            db.commit()
        except Exception:
            db.rollback()
            raise
        finally:
            db.close()

    def _row(self, row):
        if row is None:
            return None
        keys = ("lease_id", "namespace", "provider", "owner", "run_id", "run_attempt", "tested_sha",
                "expires_at", "heartbeat_at", "ttl_seconds", "heartbeat_seconds", "max_concurrency", "qps", "status")
        return dict(zip(keys, row, strict=True))

    def get(self, lease_id: str, include_expired=False):
        with self._db() as db:
            row = self._row(db.execute("SELECT * FROM leases WHERE lease_id=? AND status IN ('active','deleting')", (lease_id,)).fetchone())
        if row and not include_expired and parse_time(row["expires_at"]) <= self.clock():
            return None
        return row

    def active(self):
        with self._db() as db:
            rows = [self._row(row) for row in db.execute("SELECT * FROM leases WHERE status='active'")]
        now = self.clock()
        return [row for row in rows if (parse_time(row["expires_at"]) > now
                and (now - parse_time(row["heartbeat_at"])).total_seconds() <= row["heartbeat_seconds"] * 3)]

    def is_active(self, lease_id):
        return any(row["lease_id"] == lease_id for row in self.active())

    def create(self, body):
        required = {"lease_id", "namespace", "owner", "run_id", "run_attempt", "tested_sha", "expires_at",
                    "heartbeat_at", "ttl_seconds", "heartbeat_seconds", "max_concurrency", "qps"}
        if set(body) != required:
            raise StateError("lease request shape is invalid")
        if (re.fullmatch(r"axon_e2e_[A-Za-z0-9_]+", str(body["namespace"])) is None
                or re.fullmatch(r"[A-Za-z0-9_]+", str(body["lease_id"])) is None
                or not body["lease_id"].startswith(body["namespace"] + "_")):
            raise StateError("lease identity is outside the E2E namespace")
        if body["owner"] != "dinglebear-ai/axon" or not str(body["run_id"]).isdigit() or not str(body["run_attempt"]).isdigit():
            raise StateError("lease owner or run identity is invalid")
        if len(str(body["tested_sha"])) != 40 or any(c not in "0123456789abcdef" for c in str(body["tested_sha"])):
            raise StateError("tested SHA is invalid")
        now = self.clock(); expires = parse_time(body["expires_at"]); heartbeat = parse_time(body["heartbeat_at"])
        ttl = int(body["ttl_seconds"]); beat = int(body["heartbeat_seconds"])
        max_qps = {"qdrant": 4, "tei": 2, "llm": 2, "chrome": 2}.get(self.provider, 1)
        if ttl != 7200 or beat != 30 or expires <= now or expires > now + dt.timedelta(seconds=ttl + 5):
            raise StateError("lease time bounds are invalid")
        if (abs((heartbeat - now).total_seconds()) > 30 or int(body["max_concurrency"]) != 1
                or int(body["qps"]) <= 0 or int(body["qps"]) > max_qps):
            raise StateError("lease limits are invalid")
        with self.lock, self._db() as db, db:
            db.execute("BEGIN IMMEDIATE")
            active = db.execute("SELECT lease_id,expires_at,status FROM leases WHERE status IN ('active','deleting')").fetchall()
            for lease_id, expires_at, status in active:
                if status == "deleting":
                    raise StateError("lease cleanup is incomplete")
                if parse_time(expires_at) > now:
                    raise StateError("gateway already has an active lease")
                raise StateError("expired lease requires audited cleanup")
            try:
                db.execute("INSERT INTO leases VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?)", (
                    body["lease_id"], body["namespace"], self.provider, body["owner"], str(body["run_id"]),
                    str(body["run_attempt"]), body["tested_sha"], body["expires_at"], body["heartbeat_at"], ttl,
                    beat, 1, int(body["qps"]), "active"))
            except sqlite3.IntegrityError as error:
                raise StateError("lease identity already exists") from error
        return self.get(body["lease_id"], include_expired=True)

    @staticmethod
    def owns(row, body):
        return all(str(body.get(key, "")) == str(row[key]) for key in ("namespace", "owner", "run_id", "run_attempt"))

    def heartbeat(self, lease_id, body):
        if set(body) != {"namespace", "owner", "run_id", "run_attempt"}:
            raise StateError("heartbeat shape is invalid")
        with self.lock:
            row = self.get(lease_id)
            if row is None or row["status"] != "active" or not self.owns(row, body):
                raise StateError("lease is absent, expired, or not owned")
            now = self.clock()
            if (now - parse_time(row["heartbeat_at"])).total_seconds() > row["heartbeat_seconds"] * 3:
                raise StateError("lease heartbeat deadline elapsed")
            with self._db() as db, db:
                # expires_at is the immutable run deadline. Heartbeats prove
                # liveness within that window; they cannot make a lease immortal.
                db.execute("UPDATE leases SET heartbeat_at=? WHERE lease_id=?", (format_time(now), lease_id))
            return self.get(lease_id, include_expired=True)

    def retire(self, lease_id):
        with self._db() as db, db:
            db.execute("UPDATE leases SET status='deleted' WHERE lease_id=?", (lease_id,))

    def begin_delete(self, lease_id):
        with self._db() as db, db:
            changed = db.execute("UPDATE leases SET status='deleting' WHERE lease_id=? AND status='active'", (lease_id,)).rowcount
        if not changed and self.get(lease_id, include_expired=True) is None:
            raise StateError("lease cleanup authority is absent")

    def expired(self):
        with self._db() as db:
            rows = [self._row(row) for row in db.execute("SELECT * FROM leases WHERE status IN ('active','deleting')")]
        now = self.clock()
        return [row for row in rows if (row["status"] == "deleting" or parse_time(row["expires_at"]) <= now
                or (now - parse_time(row["heartbeat_at"])).total_seconds() > row["heartbeat_seconds"] * 3)]

    def public(self, row):
        return {key: row[key] for key in self.FIELDS}
