import subprocess
import sys
from datetime import datetime
from pathlib import Path

from src.integrations.emailer import send_email


# ============================================================
# PROJECT SETUP
# ============================================================

PROJECT_ROOT = Path(__file__).resolve().parent

PYTHON = sys.executable


# ============================================================
# RUN ONE PIPELINE STEP
# ============================================================

def run_step(
    name,
    module_name,
):
    print()
    print("=" * 60)
    print(name)
    print("=" * 60)

    result = subprocess.run(
        [
            PYTHON,
            "-m",
            module_name,
        ],
        cwd=PROJECT_ROOT,
        capture_output=True,
        text=True,
    )

    stdout = (
        result.stdout.strip()
    )

    stderr = (
        result.stderr.strip()
    )

    # Show subprocess output in the main log.
    if stdout:
        print(stdout)

    if stderr:
        print(stderr)

    log = (
        f"{name}\n"
        f"{'-' * len(name)}\n"
    )

    if stdout:
        log += (
            f"\n{stdout}\n"
        )

    if stderr:
        log += (
            "\nERROR OUTPUT:\n"
            f"{stderr}\n"
        )

    if result.returncode != 0:
        raise RuntimeError(
            f"{name} failed with exit code "
            f"{result.returncode}\n\n"
            f"{log}"
        )

    return log


# ============================================================
# SEND REPORT
# ============================================================

def send_pipeline_report(
    success,
    body,
):
    date_text = datetime.now().strftime(
        "%d %b %Y"
    )

    if success:
        subject = (
            "CareerPilot Daily Report "
            f"- {date_text} - SUCCESS"
        )

    else:
        subject = (
            "CareerPilot Daily Report "
            f"- {date_text} - FAILED"
        )

    try:
        send_email(
            subject=subject,
            body=body,
        )

        print()
        print(
            "Daily report email sent."
        )

    except Exception as exc:
        # Email failure should not hide
        # the actual pipeline result.
        print()
        print(
            "Warning: Could not send "
            f"daily report email: {exc}"
        )


# ============================================================
# PIPELINE
# ============================================================

def main():

    started_at = datetime.now()

    logs = []

    try:

        # ----------------------------------------------------
        # STEP 1 - SEEK
        # ----------------------------------------------------

        scraper_log = run_step(
            "1. SEEK SCRAPER",
            "src.scraper.run_scraper",
        )

        logs.append(
            scraper_log
        )

        # ----------------------------------------------------
        # STEP 2 - RELEVANCE
        # ----------------------------------------------------

        relevance_log = run_step(
            "2. RELEVANCE ANALYSIS",
            "src.intelligence.relevance",
        )

        logs.append(
            relevance_log
        )

        # ----------------------------------------------------
        # SUCCESS
        # ----------------------------------------------------

        finished_at = datetime.now()

        duration = (
            finished_at
            - started_at
        )

        report = (
            "CareerPilot daily pipeline completed "
            "successfully.\n\n"
            f"Started: "
            f"{started_at.strftime('%d %b %Y %H:%M:%S')}\n"
            f"Finished: "
            f"{finished_at.strftime('%d %b %Y %H:%M:%S')}\n"
            f"Duration: {duration}\n\n"
            "========================================\n\n"
            + "\n\n".join(logs)
        )

        print()
        print("=" * 60)
        print(
            "CareerPilot daily pipeline complete"
        )
        print("=" * 60)

        send_pipeline_report(
            success=True,
            body=report,
        )

    except Exception as exc:

        finished_at = datetime.now()

        duration = (
            finished_at
            - started_at
        )

        report = (
            "CareerPilot daily pipeline FAILED.\n\n"
            f"Started: "
            f"{started_at.strftime('%d %b %Y %H:%M:%S')}\n"
            f"Failed: "
            f"{finished_at.strftime('%d %b %Y %H:%M:%S')}\n"
            f"Duration: {duration}\n\n"
            "ERROR\n"
            "=====\n"
            f"{exc}\n\n"
        )

        if logs:
            report += (
                "COMPLETED STEPS\n"
                "===============\n\n"
                + "\n\n".join(logs)
            )

        print()
        print("=" * 60)
        print(
            "CareerPilot pipeline FAILED"
        )
        print("=" * 60)

        print(exc)

        send_pipeline_report(
            success=False,
            body=report,
        )

        raise


if __name__ == "__main__":
    main()