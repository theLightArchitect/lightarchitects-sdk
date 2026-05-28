"""Quick connectivity check before running the full eval suite."""
import asyncio, httpx, os, sys

BASE_URL = os.environ.get("LIGHTARCHITECTS_BASE_URL", "http://localhost:8799")
TOKEN    = os.environ.get("LIGHTARCHITECTS_WEBSHELL_TOKEN", "test-token")

async def main():
    async with httpx.AsyncClient() as c:
        # 1. Health
        r = await c.get(f"{BASE_URL}/api/health", timeout=5)
        print(f"health: {r.status_code}")
        assert r.status_code == 200, f"health failed: {r.status_code}"

        # 2. Auth
        r = await c.get(
            f"{BASE_URL}/api/builds",
            headers={"Authorization": f"Bearer {TOKEN}"},
            timeout=5,
        )
        print(f"auth:   {r.status_code}")
        assert r.status_code == 200, f"auth failed — wrong token? {r.text[:200]}"

        # 3. Create a build
        r = await c.post(
            f"{BASE_URL}/api/builds",
            json={"cwd": str(os.path.expanduser("~"))},
            headers={"Authorization": f"Bearer {TOKEN}", "Content-Type": "application/json"},
            timeout=5,
        )
        print(f"create build: {r.status_code}")
        assert r.status_code == 200, f"build creation failed: {r.text[:200]}"
        build_id = r.json()["build_id"]
        print(f"build_id: {build_id}")

    print("\nServer check PASSED — ready to run eval suite.")

asyncio.run(main())
