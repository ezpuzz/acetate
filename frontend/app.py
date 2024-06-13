import os
from flask import Flask, render_template, request, session, redirect, url_for
from flask_htmx import HTMX

from authlib.integrations.flask_client import OAuth
from jinja2 import StrictUndefined
import requests

app = Flask(__name__, static_url_path="/public")
app.jinja_env.undefined = StrictUndefined

app.secret_key = os.environ.get("FLASK_SECRET_KEY", "supersekrit")


htmx = HTMX(app)

oauth = OAuth(app)

oauth.register(
    name="discogs",
    client_id="iHrDPobqsnneJtaewqfv",
    client_secret="kUXeSihKsZVkgifXWKBXQanHdfIQBPXp",
    request_token_url="https://api.discogs.com/oauth/request_token",
    access_token_url="https://api.discogs.com/oauth/access_token",
    authorize_url="https://www.discogs.com/oauth/authorize",
    fetch_token=lambda: session.get("token"),  # DON'T DO IT IN PRODUCTION
)


@app.route("/")
def index():
    releases = requests.get("http://localhost:3000/releases", params=request.args)
    releases = [r["_source"] for r in releases.json()["hits"]["hits"]]

    for r in releases:
        if "videos" in r:
            r["videos"] = [v[32:] for v in r["videos"]]

    return render_template(
        "index.jinja",
        **{
            "pageSize": 25,
            "releases": releases,
            **request.args,
        },
    )


@app.route("/releases")
def releases():
    releases = requests.get(
        "http://localhost:3000/releases",
        params=request.args,
    )
    releases.raise_for_status()
    print(releases.json())
    releases = [r["_source"] for r in releases.json()["hits"]["hits"]]

    for r in releases:
        if "videos" in r:
            r["videos"] = [v[32:] for v in r["videos"]]

    if htmx and not htmx.boosted:
        return render_template("releases.jinja", releases=releases)
    return render_template(
        "index.jinja",
        **{
            "pageSize": 25,
            "releases": releases,
            **request.args,
        },
    )


@app.route("/wants")
def wants():
    wants = oauth.discogs.get(
        f"https://api.discogs.com/users/{session["user"]["username"]}/wants"
    )
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
