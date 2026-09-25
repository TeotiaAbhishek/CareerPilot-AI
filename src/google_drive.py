import io
import re
import sys
from pathlib import Path

from google.auth.transport.requests import Request
from google.oauth2.credentials import Credentials
from google_auth_oauthlib.flow import InstalledAppFlow
from googleapiclient.discovery import build
from googleapiclient.http import (
    MediaIoBaseDownload,
    MediaIoBaseUpload,
)

from database import (
    get_job_documents,
    get_job_for_document,
    save_document_drive_files,
)

from document_formatter import (
    markdown_to_resume_docx,
    markdown_to_cover_letter_docx,
)

# ============================================================
# PATHS
# ============================================================

PROJECT_ROOT = Path(__file__).resolve().parent.parent

GOOGLE_CREDENTIALS_PATH = (
    PROJECT_ROOT
    / "config"
    / "google_credentials.json"
)

GOOGLE_TOKEN_PATH = (
    PROJECT_ROOT
    / "config"
    / "google_token.json"
)


# ============================================================
# GOOGLE SETTINGS
# ============================================================

SCOPES = [
    "https://www.googleapis.com/auth/drive.file"
]

ROOT_FOLDER_NAME = "CareerPilot AI"


# ============================================================
# AUTHENTICATION
# ============================================================

def get_drive_service():

    credentials = None

    if GOOGLE_TOKEN_PATH.exists():

        credentials = (
            Credentials.from_authorized_user_file(
                GOOGLE_TOKEN_PATH,
                SCOPES,
            )
        )

    if (
        not credentials
        or not credentials.valid
    ):

        if (
            credentials
            and credentials.expired
            and credentials.refresh_token
        ):

            credentials.refresh(
                Request()
            )

        else:

            if not GOOGLE_CREDENTIALS_PATH.exists():

                raise FileNotFoundError(
                    "Google OAuth credentials not found at "
                    f"{GOOGLE_CREDENTIALS_PATH}"
                )

            flow = (
                InstalledAppFlow
                .from_client_secrets_file(
                    GOOGLE_CREDENTIALS_PATH,
                    SCOPES,
                )
            )

            credentials = (
                flow.run_local_server(
                    port=0,
                    open_browser=False,
                )
            )

        GOOGLE_TOKEN_PATH.write_text(
            credentials.to_json(),
            encoding="utf-8",
        )

    return build(
        "drive",
        "v3",
        credentials=credentials,
    )


# ============================================================
# FILE NAME CLEANING
# ============================================================

def clean_file_name(value):

    value = str(value or "")

    value = re.sub(
        r"[\r\n\t]+",
        " ",
        value,
    )

    value = re.sub(
        r"\s+",
        " ",
        value,
    )

    value = value.replace(
        "/",
        "-"
    )

    value = value.replace(
        "\\",
        "-"
    )

    return value.strip()


# ============================================================
# CAREERPILOT FOLDER
# ============================================================

def get_or_create_root_folder(service):

    query = (
        f"name = '{ROOT_FOLDER_NAME}' "
        "and mimeType = "
        "'application/vnd.google-apps.folder' "
        "and trashed = false"
    )

    result = (
        service.files()
        .list(
            q=query,
            spaces="drive",
            fields="files(id, name)",
            pageSize=10,
        )
        .execute()
    )

    folders = result.get(
        "files",
        []
    )

    if folders:

        return folders[0]["id"]

    metadata = {
        "name": ROOT_FOLDER_NAME,
        "mimeType": (
            "application/"
            "vnd.google-apps.folder"
        ),
    }

    folder = (
        service.files()
        .create(
            body=metadata,
            fields="id",
        )
        .execute()
    )

    return folder["id"]


# ============================================================
# DELETE OLD GENERATED FILE
# ============================================================

def safe_delete_file(
    service,
    file_id,
):

    if not file_id:
        return

    try:

        service.files().delete(
            fileId=file_id
        ).execute()

    except Exception as exc:

        print(
            "Warning: could not delete "
            f"old Drive file {file_id}: "
            f"{exc}"
        )


# ============================================================
# MARKDOWN -> GOOGLE DOC
# ============================================================

def create_google_doc(
    service,
    markdown,
    file_name,
    folder_id,
    document_type,
):

    file_metadata = {
        "name": file_name,
        "mimeType": (
            "application/"
            "vnd.google-apps.document"
        ),
        "parents": [
            folder_id
        ],
    }


    if document_type == "resume":
        
        source_buffer = (
            markdown_to_resume_docx(
                markdown
            )
        )
    
    elif document_type == "cover":
    
        source_buffer = (
            markdown_to_cover_letter_docx(
                markdown
            )
        )
    
    else:
    
        raise ValueError(
            "document_type must be "
            "'resume' or 'cover'"
        )
    
    
    source_mime = (
        "application/"
        "vnd.openxmlformats-officedocument."
        "wordprocessingml.document"
    )


    media = MediaIoBaseUpload(
        source_buffer,
        mimetype=source_mime,
        resumable=False,
    )


    result = (
        service.files()
        .create(
            body=file_metadata,
            media_body=media,
            fields="id, webViewLink",
        )
        .execute()
    )


    doc_id = result["id"]

    doc_url = (
        result.get(
            "webViewLink"
        )
        or
        (
            "https://docs.google.com/"
            f"document/d/{doc_id}/edit"
        )
    )


    return (
        doc_id,
        doc_url,
    )


# ============================================================
# GOOGLE DOC -> PDF BYTES
# ============================================================

def export_google_doc_to_pdf(
    service,
    doc_id,
):

    request = (
        service.files()
        .export_media(
            fileId=doc_id,
            mimeType="application/pdf",
        )
    )

    buffer = io.BytesIO()

    downloader = MediaIoBaseDownload(
        buffer,
        request,
    )

    done = False

    while not done:

        _, done = (
            downloader.next_chunk()
        )

    buffer.seek(0)

    return buffer


# ============================================================
# UPLOAD PDF
# ============================================================

def upload_pdf(
    service,
    pdf_buffer,
    file_name,
    folder_id,
):

    metadata = {
        "name": file_name,
        "parents": [
            folder_id
        ],
    }

    media = MediaIoBaseUpload(
        pdf_buffer,
        mimetype="application/pdf",
        resumable=False,
    )

    result = (
        service.files()
        .create(
            body=metadata,
            media_body=media,
            fields="id, webViewLink",
        )
        .execute()
    )

    pdf_id = result["id"]

    pdf_url = (
        result.get(
            "webViewLink"
        )
        or
        (
            "https://drive.google.com/"
            f"file/d/{pdf_id}/view"
        )
    )

    return (
        pdf_id,
        pdf_url,
    )


# ============================================================
# PUBLISH SAVED DOCUMENT
# ============================================================

def publish_document(
    job_id,
    document_type,
):

    if document_type not in {
        "resume",
        "cover",
    }:

        raise ValueError(
            "document_type must be "
            "'resume' or 'cover'"
        )

    job = get_job_for_document(
        job_id
    )

    if not job:

        raise ValueError(
            f"Job {job_id} not found."
        )

    documents = (
        get_job_documents(
            job_id
        )
        or {}
    )


    # --------------------------------------------------------
    # CHOOSE MARKDOWN + OLD FILE IDS
    # --------------------------------------------------------

    if document_type == "resume":

        markdown = documents.get(
            "resume_markdown"
        )

        old_doc_id = documents.get(
            "resume_doc_id"
        )

        old_pdf_id = documents.get(
            "resume_pdf_id"
        )

        document_label = "Resume"

    else:

        markdown = documents.get(
            "cover_letter_markdown"
        )

        old_doc_id = documents.get(
            "cover_letter_doc_id"
        )

        old_pdf_id = documents.get(
            "cover_letter_pdf_id"
        )

        document_label = (
            "Cover Letter"
        )


    if not markdown:

        raise ValueError(
            f"No saved {document_label} "
            f"Markdown found for job {job_id}."
        )


    # --------------------------------------------------------
    # GOOGLE DRIVE
    # --------------------------------------------------------

    service = get_drive_service()

    folder_id = (
        get_or_create_root_folder(
            service
        )
    )


    # --------------------------------------------------------
    # REMOVE OLD VERSION ON REGENERATION
    # --------------------------------------------------------

    safe_delete_file(
        service,
        old_doc_id,
    )

    safe_delete_file(
        service,
        old_pdf_id,
    )


    # --------------------------------------------------------
    # FILE NAMES
    # --------------------------------------------------------

    company = clean_file_name(
        job.get("company")
        or "Company"
    )

    title = clean_file_name(
        job.get("title")
        or "Role"
    )

    base_name = (
        f"{company} - "
        f"{title} - "
        f"{document_label}"
    )


    # --------------------------------------------------------
    # CREATE GOOGLE DOC
    # --------------------------------------------------------

    print(
        f"Creating Google Doc: "
        f"{base_name}"
    )

    doc_id, doc_url = create_google_doc(
        service=service,
        markdown=markdown,
        file_name=base_name,
        folder_id=folder_id,
            document_type=document_type,
    )

    # --------------------------------------------------------
    # EXPORT PDF
    # --------------------------------------------------------

    print(
        "Exporting PDF..."
    )

    pdf_buffer = (
        export_google_doc_to_pdf(
            service,
            doc_id,
        )
    )


    # --------------------------------------------------------
    # UPLOAD PDF
    # --------------------------------------------------------

    pdf_id, pdf_url = (
        upload_pdf(
            service=service,
            pdf_buffer=pdf_buffer,
            file_name=(
                f"{base_name}.pdf"
            ),
            folder_id=folder_id,
        )
    )


    # --------------------------------------------------------
    # SAVE DRIVE METADATA
    # --------------------------------------------------------

    save_document_drive_files(
        job_id=job_id,
        document_type=document_type,
        doc_id=doc_id,
        doc_url=doc_url,
        pdf_id=pdf_id,
        pdf_url=pdf_url,
    )


    return {
        "doc_id": doc_id,
        "doc_url": doc_url,
        "pdf_id": pdf_id,
        "pdf_url": pdf_url,
    }


# ============================================================
# COMMAND-LINE ENTRY POINT
# ============================================================

if __name__ == "__main__":

    if len(sys.argv) != 3:

        print(
            "Usage:"
            "\n"
            "python src/google_drive.py "
            "resume <job_id>"
            "\n"
            "python src/google_drive.py "
            "cover <job_id>"
        )

        raise SystemExit(1)


    document_type = (
        sys.argv[1]
        .strip()
        .lower()
    )

    job_id = (
        sys.argv[2]
        .strip()
    )


    result = publish_document(
        job_id=job_id,
        document_type=document_type,
    )


    print(
        "\nDone!"
    )

    print(
        f'Google Doc: '
        f'{result["doc_url"]}'
    )

    print(
        f'PDF: '
        f'{result["pdf_url"]}'
    )