import os
from flask import Flask, render_template, request, session, redirect, url_for
from flask_htmx import HTMX

from authlib.integrations.flask_client import OAuth
from jinja2 import StrictUndefined
import requests

app = Flask(__name__, static_url_path="/public")
app.jinja_env.undefined = StrictUndefined

app.secret_key = os.environ.get("FLASK_SECRET_KEY", "supersekrit")

AXUM_API = os.environ.get("AXUM_API", "https://acetate.onrender.com/")

htmx = HTMX(app)

oauth = OAuth(app)

oauth.register(
    name="discogs",
    client_id=os.environ.get("DISCOGS_CLIENT_ID", "iHrDPobqsnneJtaewqfv"),
    client_secret=os.environ.get(
        "DISCOGS_CLIENT_SECRET", "kUXeSihKsZVkgifXWKBXQanHdfIQBPXp"
    ),
    request_token_url="https://api.discogs.com/oauth/request_token",
    access_token_url="https://api.discogs.com/oauth/access_token",
    authorize_url="https://www.discogs.com/oauth/authorize",
    fetch_token=lambda: session.get("token"),  # DON'T DO IT IN PRODUCTION
)


@app.route("/")
@app.route("/releases")
def releases():
    filters = requests.get(f"{AXUM_API}filters", timeout=5)
    filters.raise_for_status()

    releases = requests.get(
        f"{AXUM_API}releases",
        params=[
            p
            for p in [
                ("field", "nested:labels.name")
                if "label" in request.args and request.args["label"]
                else None,
                ("value", request.args["label"])
                if "label" in request.args and request.args["label"]
                else None,
                ("field", "nested:tracklist.title")
                if "song" in request.args and request.args["song"]
                else None,
                ("value", request.args["song"])
                if "song" in request.args and request.args["song"]
                else None,
                ("field", "nested:artists.name")
                if "artist" in request.args and request.args["artist"]
                else None,
                ("value", request.args["artist"])
                if "artist" in request.args and request.args["artist"]
                else None,
                ("from", request.args.get("from", None)),
                ("field", "formats.name.keyword")
                if "formats.name.keyword" in request.args
                else None,
                ("value", request.args["formats.name.keyword"])
                if "formats.name.keyword" in request.args
                else None,
                ("field", "formats.descriptions.keyword")
                if "formats.descriptions.keyword" in request.args
                else None,
                ("value", request.args["formats.descriptions.keyword"])
                if "formats.descriptions.keyword" in request.args
                else None,
                ("field", "styles.keyword")
                if "styles.keyword" in request.args
                else None,
                ("value", request.args["styles.keyword"])
                if "styles.keyword" in request.args
                else None,
            ]
            if p is not None
        ],
        timeout=10,
    )
    print(releases.json())
    releases.raise_for_status()
    releases = releases.json()

    hits = releases["hits"]["total"]["value"]

    releases = [r["_source"] for r in releases["hits"]["hits"]]

    for r in releases:
        if "videos" in r:
            r["videos"] = [v[32:] for v in r["videos"]]

    pageSize = int(request.args.get("pageSize", 5))
    offset = int(request.args.get("from", 0))
    page = 1 + offset // pageSize

    if htmx and not htmx.boosted:
        return render_template(
            "releases/results.jinja",
            **{
                "pageSize": pageSize,
                "releases": releases,
                "hits": hits,
                "page": page,
                "from": offset,
                "filters": filters.json(),
                **request.args,
            },
        )
    return render_template(
        "index.jinja",
        **{
            "pageSize": pageSize,
            "releases": releases,
            "hits": hits,
            "page": page,
            "from": offset,
            "filters": filters.json(),
            **request.args,
        },
    )


@app.route("/wants")
def wants():
    username = session["user"]["username"]
    wants = oauth.discogs.get(f"https://api.discogs.com/users/{username}/wants")
    wants.raise_for_status()
    wants = wants.json()
    return render_template(
        "wants.jinja", **{"pageSize": 25, "wants": wants, **request.args}
    )


@app.route("/login")
def login():
    redirect_uri = url_for("auth", _external=True)
    return oauth.discogs.authorize_redirect(redirect_uri)


@app.route("/auth")
def auth():
    token = oauth.discogs.authorize_access_token()
    resp = oauth.discogs.get("https://api.discogs.com/oauth/identity")
    user = resp.json()
    # DON'T DO IT IN PRODUCTION, SAVE INTO DB IN PRODUCTION
    session["token"] = token
    session["user"] = user
    return redirect("/")
