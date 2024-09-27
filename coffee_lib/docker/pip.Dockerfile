FROM python:3.10-slim

COPY requirements.txt /plugin/requirements.txt

WORKDIR /plugin

RUN pip install -r requirements.txt --no-cache-dir

COPY . /plugin/

CMD [ "" ]