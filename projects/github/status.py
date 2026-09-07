"""Read a bounded GitHub pull-request snapshot through the user's existing gh login."""

from datetime import datetime, timezone
import json
import os
import re
import selectors
import signal
import subprocess
import sys
import time
from urllib.parse import urlsplit

LIMIT = 20
MAX_OUTPUT = 256 * 1024
QUERY = """
query SeelePullRequests($reviews: String!) {
  viewer { login pullRequests(first: 20, states: OPEN, orderBy: {field: UPDATED_AT, direction: DESC}) {
    totalCount pageInfo { hasNextPage } nodes { ...Pull }
  } }
  search(query: $reviews, type: ISSUE, first: 20) {
    issueCount pageInfo { hasNextPage } nodes { ... on PullRequest { ...Pull } }
  }
}
fragment Pull on PullRequest {
  number title url updatedAt isDraft reviewDecision repository { nameWithOwner }
  commits(last: 1) { nodes { commit { statusCheckRollup { state } } } }
}
"""


class FetchError(Exception):
    def __init__(self, state, message):
        self.state, self.message = state, message


def failure(text):
    lower = text.lower()
    if "rate limit" in lower or "rate_limit" in lower or "http 429" in lower:
        return FetchError("rate-limited", "GitHub's rate limit was reached. Automatic refresh will wait five minutes.")
    if any(token in lower for token in ["http 401", "bad credentials", "authentication", "gh auth login", "not logged"]):
        return FetchError("auth-required", "Sign in with GitHub CLI, then refresh.")
    return FetchError("error", "GitHub could not be reached. Check your connection and account access, then refresh.")


def run_gh(arguments, timeout=15, max_output=MAX_OUTPUT):
    environment = dict(os.environ, GH_PROMPT_DISABLED="1", GH_PAGER="cat", NO_COLOR="1")
    # Debug diagnostics may contain account details; never forward them to QML.
    environment.pop("GH_DEBUG", None)
    try:
        process = subprocess.Popen(["gh", *arguments], stdin=subprocess.DEVNULL,
                                   stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                                   env=environment, start_new_session=True)
    except FileNotFoundError:
        raise FetchError("error", "GitHub CLI is unavailable.") from None
    output, errors = bytearray(), bytearray()
    deadline = time.monotonic() + timeout
    try:
        with selectors.DefaultSelector() as streams:
            streams.register(process.stdout, selectors.EVENT_READ, output)
            streams.register(process.stderr, selectors.EVENT_READ, errors)
            while streams.get_map():
                remaining = deadline - time.monotonic()
                if remaining <= 0:
                    raise FetchError("error", "GitHub took too long to respond. Try refreshing again.")
                for key, _ in streams.select(remaining):
                    chunk = os.read(key.fd, 16384)
                    if not chunk:
                        streams.unregister(key.fileobj)
                        continue
                    if len(output) + len(errors) + len(chunk) > max_output:
                        raise FetchError("error", "GitHub returned more data than this panel can display.")
                    key.data.extend(chunk)
        process.wait(timeout=max(0.01, deadline - time.monotonic()))
        if process.returncode:
            raise failure(errors.decode("utf-8", errors="replace"))
        try:
            return json.loads(output)
        except (ValueError, UnicodeError):
            raise FetchError("error", "GitHub returned an unreadable response.") from None
    except subprocess.TimeoutExpired:
        raise FetchError("error", "GitHub took too long to respond. Try refreshing again.") from None
    finally:
        if process.poll() is None:
            try:
                os.killpg(process.pid, signal.SIGKILL)
            except ProcessLookupError:
                pass
        process.wait()
        process.stdout.close()
        process.stderr.close()


def hostname(value):
    value = str(value).lower()
    if not re.fullmatch(r"[a-z0-9](?:[a-z0-9.-]{0,251}[a-z0-9])?", value) or ".." in value:
        raise FetchError("error", "The configured GitHub hostname is invalid.")
    return value


def safe_url(value, host):
    if not isinstance(value, str) or len(value) > 1024:
        return ""
    try:
        parsed = urlsplit(value)
        if (parsed.scheme != "https" or parsed.hostname != host or parsed.username is not None
                or parsed.password is not None or parsed.port is not None or parsed.query or parsed.fragment
                or not re.fullmatch(r"/[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+/pull/[1-9][0-9]*", parsed.path)):
            return ""
    except ValueError:
        return ""
    return value


def plain(value, limit=256):
    return "".join(char if char.isprintable() else " " for char in str(value or ""))[:limit]


def pull(item, host):
    if not isinstance(item, dict):
        return None
    url = safe_url(item.get("url"), host)
    if not url or type(item.get("number")) is not int or item["number"] <= 0:
        return None
    commits = ((item.get("commits") or {}).get("nodes") or [])
    check = "UNKNOWN"
    if commits and isinstance(commits[-1], dict):
        rollup = (commits[-1].get("commit") or {}).get("statusCheckRollup") or {}
        if rollup.get("state") in ("SUCCESS", "FAILURE", "PENDING", "ERROR", "EXPECTED"):
            check = rollup["state"]
    review = item.get("reviewDecision")
    if review not in ("APPROVED", "CHANGES_REQUESTED", "REVIEW_REQUIRED"):
        review = ""
    return {"number": item["number"], "title": plain(item.get("title")), "url": url,
            "repository": plain((item.get("repository") or {}).get("nameWithOwner"), 200),
            "updatedAt": plain(item.get("updatedAt"), 32), "draft": item.get("isDraft") is True,
            "checks": check, "review": review}


def connection(value, host, count_name):
    if not isinstance(value, dict) or not isinstance(value.get("nodes"), list):
        raise FetchError("error", "GitHub returned incomplete pull-request data.")
    entries, seen = [], set()
    for item in value["nodes"][:LIMIT]:
        entry = pull(item, host)
        if entry is not None and entry["url"] not in seen:
            entries.append(entry)
            seen.add(entry["url"])
    total = value.get(count_name)
    if type(total) is not int or total < len(entries):
        total = len(entries)
    return entries, total


def snapshot(run=run_gh, host=None):
    host = hostname(host or os.environ.get("SEELE_GITHUB_HOST", "github.com"))
    identity = run(["api", "--hostname", host, "user"])
    login = identity.get("login", "") if isinstance(identity, dict) else ""
    if not isinstance(login, str) or not re.fullmatch(r"[A-Za-z0-9-]{1,64}", login):
        raise FetchError("error", "GitHub did not identify the current account.")
    result = run(["api", "--hostname", host, "graphql", "-f", "query=" + QUERY,
                  "-f", f"reviews=is:pr is:open review-requested:{login} sort:updated-desc"])
    if not isinstance(result, dict):
        raise FetchError("error", "GitHub returned an unreadable response.")
    if result.get("errors"):
        messages = " ".join(str(item.get("message", "")) for item in result["errors"] if isinstance(item, dict))
        raise failure(messages)
    data = result.get("data") or {}
    viewer = data.get("viewer") or {}
    if viewer.get("login") != login:
        raise FetchError("error", "The GitHub account changed during refresh. Refresh again.")
    authored, authored_total = connection(viewer.get("pullRequests"), host, "totalCount")
    reviews, review_total = connection(data.get("search"), host, "issueCount")
    return {"state": "ready", "message": "", "host": host, "viewer": login,
            "updatedAt": datetime.now(timezone.utc).isoformat(timespec="seconds"),
            "authored": authored, "reviews": reviews,
            "authoredTotal": authored_total, "reviewTotal": review_total}


def stop(signum, _frame):
    # Raising unwinds run_gh's finally block; default SIGTERM would strand its
    # separate process group when Quickshell destroys the worker on reload.
    raise SystemExit(128 + signum)


def main():
    signal.signal(signal.SIGTERM, stop)
    signal.signal(signal.SIGINT, stop)
    try:
        result = snapshot()
    except FetchError as error:
        result = {"state": error.state, "message": error.message}
    except (OSError, ValueError, TypeError, AttributeError, KeyError):
        result = {"state": "error", "message": "GitHub returned incomplete data. Try refreshing again."}
    print(json.dumps(result, ensure_ascii=True))


if __name__ == "__main__":
    main()
