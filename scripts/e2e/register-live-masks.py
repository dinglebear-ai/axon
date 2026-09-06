#!/usr/bin/env python3
import os
values=os.environ.get("TOKENS","").splitlines()
if not values or any(not value.strip() for value in values):raise SystemExit("provider application credentials unavailable")
for value in values:print(f"::add-mask::{value}")
