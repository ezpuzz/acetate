import os
from flask import Flask, render_template, request, session, redirect, url_for
from flask_htmx import HTMX
from flask_sqlalchemy import SQLAlchemy

from authlib.integrations.flask_client import OAuth
from jinja2 import StrictUndefined
import requests

from dotenv import load_dotenv

load_dotenv()

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


class Action(db.Model):
    __table__ = db.metadata.tables["actions"]


htmx = HTMX(app)

oauth = OAuth(app)


def get_token():
    user = db.session.scalar(
        db.select(User).where(User.discogs_user_id == session.get("user").get("id"))
    )
    print(user)
    return dict(
        oauth_token=user.discogs_oauth_token,
        oauth_token_secret=user.discogs_oauth_token_secret,
    )


oauth.register(
    name="discogs",
    client_id=os.environ.get("DISCOGS_CLIENT_ID", None),
    client_secret=os.environ.get("DISCOGS_CLIENT_SECRET", None),
    request_token_url="https://api.discogs.com/oauth/request_token",
    access_token_url="https://api.discogs.com/oauth/access_token",
    authorize_url="https://www.discogs.com/oauth/authorize",
    fetch_token=get_token,
)


@app.route("/")
@app.route("/releases")
def releases():
    print(session.get("user"))
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
                (
                    "videos_only",
                    "true"
                    if request.args.get("videos_only", "on") == "on"
                    else "false",
                ),
                *[
                    x
                    for y in [
                        [
                            ("field", "formats.name.keyword"),
                            ("value", v),
                        ]
                        for v in request.args.getlist("formats.name.keyword")
                    ]
                    for x in y
                ],
                *[
                    x
                    for y in [
                        [
                            ("field", "formats.descriptions.keyword"),
                            ("value", v),
                        ]
                        for v in request.args.getlist("formats.descriptions.keyword")
                    ]
                    for x in y
                ],
                *[
                    x
                    for y in [
                        [("field", "styles.keyword"), ("value", v)]
                        for v in request.args.getlist("styles.keyword")
                    ]
                    for x in y
                ],
            ]
            if p is not None
        ],
        timeout=10,
    )
    print(releases.url)
    releases.raise_for_status()
    releases = releases.json()

    hits = int(releases["hits"]["total"]["value"])

    releases = [r["_source"] for r in releases["hits"]["hits"]]

    for r in releases:
        if "videos" in r:
            r["videos"] = [v[32:] for v in r["videos"]]

        if "user" in session:
            r["thumb"] = (
                oauth.discogs.get(f"https://api.discogs.com/releases/{r['id']}")
                .json()
                .get("thumb")
            )

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


@app.post("/want")
def want():
    if not request.form.get("release_id"):
        raise Exception()

    username = session["user"]["username"]

    wants = oauth.discogs.put(
        f"https://api.discogs.com/users/{username}/wants/{request.form.get('release_id')}"
    )
    wants.raise_for_status()
    return render_template("releases/wanted.jinja")


@app.post("/hide")
def hide():
    if not request.form.get("release_id"):
        raise Exception()

    user = db.session.scalar(
        db.select(User).where(User.discogs_user_id == session.get("user").get("id"))
    )

    action = Action(
        user_id=user.user_id, action="HIDE", identifier=request.form.get("release_id")
    )

    db.session.add(action)
    db.session.commit()
    return render_template("releases/hidden.jinja")


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
