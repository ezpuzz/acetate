import os
from flask import Flask, render_template, request, session, redirect, url_for
from flask_htmx import HTMX
from flask_sqlalchemy import SQLAlchemy

from authlib.integrations.flask_client import OAuth
from jinja2 import StrictUndefined
import requests

db = SQLAlchemy()

app = Flask(__name__, static_url_path="/public")
app.jinja_env.undefined = StrictUndefined

app.config["SQLALCHEMY_DATABASE_URI"] = os.environ.get("DATABASE_URL")

app.secret_key = os.environ.get("FLASK_SECRET_KEY", "supersekrit")

AXUM_API = os.environ.get("AXUM_API", "https://acetate.onrender.com/")

db.init_app(app)

with app.app_context():
    db.reflect()


class User(db.Model):
    __table__ = db.metadata.tables["users"]


htmx = HTMX(app)

oauth = OAuth(app)

oauth.register(
    name="discogs",
    client_id=os.environ.get("DISCOGS_CLIENT_ID", None),
    client_secret=os.environ.get("DISCOGS_CLIENT_SECRET", None),
    request_token_url="https://api.discogs.com/oauth/request_token",
    access_token_url="https://api.discogs.com/oauth/access_token",
    authorize_url="https://www.discogs.com/oauth/authorize",
    fetch_token=lambda: session.get("token"),  # DON'T DO IT IN PRODUCTION
)


@app.route("/")
@app.route("/releases")
def releases():
    print(session["user"])
    filters = requests.get(f"{AXUM_API}filters", timeout=5)
    filters.raise_for_status()

    pageSize = int(request.args.get("pageSize", 5))
    offset = (int(request.args.get("page", 1)) - 1) * pageSize
    page = 1 + offset // pageSize

    print(pageSize)
    print(offset)

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
                ("from", offset),
                ("size", pageSize),
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
    print(releases.url)
    releases.raise_for_status()
    releases = releases.json()

    hits = releases["hits"]["total"]["value"]

    releases = [r["_source"] for r in releases["hits"]["hits"]]

    for r in releases:
        if "videos" in r:
            r["videos"] = [v[32:] for v in r["videos"]]

    print(session)
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


@app.route("/users")
def users():
    print(session)
    db_user = User(discogs_access_token="asdf", discogs_refresh_token=None)
    db.session.add(db_user)
    db.session.commit()

    u = db.session.execute(db.select(User)).scalars().all()
    print(u)
    return u


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


@app.route("/logout")
def logout():
    session.clear()
    return redirect("/")


from sqlalchemy.dialects.postgresql import insert


@app.route("/auth")
def auth():
    token = oauth.discogs.authorize_access_token()
    resp = oauth.discogs.get("https://api.discogs.com/oauth/identity")
    user = resp.json()

    print(token)
    print(user)
    print(session)

    stmt = insert(User).values(
        discogs_oauth_token=token["oauth_token"],
        discogs_oauth_token_secret=token["oauth_token_secret"],
        discogs_user_id=user["id"],
    )
    stmt = stmt.on_conflict_do_update(
        constraint="users_discogs_user_id_key",
        set_=dict(
            discogs_oauth_token=token["oauth_token"],
            discogs_oauth_token_secret=token["oauth_token_secret"],
        ),
    )
    print(stmt)
    db.session.execute(stmt)
    db.session.commit()

    session["user"] = user

    return redirect("/")
