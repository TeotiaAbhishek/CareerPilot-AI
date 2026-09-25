import os
import smtplib

from email.message import EmailMessage
from dotenv import load_dotenv
from pathlib import Path


PROJECT_ROOT = Path(__file__).resolve().parent.parent

load_dotenv(
    PROJECT_ROOT / ".env",
    override=True,
)


def send_email(
    subject,
    body,
):
    sender = os.getenv(
        "EMAIL_ADDRESS"
    )

    password = os.getenv(
        "EMAIL_APP_PASSWORD"
    )

    receiver = os.getenv(
        "EMAIL_TO"
    )

    if not all([
        sender,
        password,
        receiver,
    ]):
        raise ValueError(
            "Email configuration missing."
        )

    message = EmailMessage()

    message["From"] = sender
    message["To"] = receiver
    message["Subject"] = subject

    message.set_content(
        body
    )

    with smtplib.SMTP_SSL(
        "smtp.gmail.com",
        465,
        timeout=30,
    ) as smtp:

        smtp.login(
            sender,
            password,
        )

        smtp.send_message(
            message
        )