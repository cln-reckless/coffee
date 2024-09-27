FROM python:3.10-slim

RUN pip3 install poetry

COPY pyproject.toml /plugin/pyproject.toml

WORKDIR /plugin

RUN poetry config virtualenvs.create false && poetry install --without dev

COPY . /plugin/

CMD [ "python" ]