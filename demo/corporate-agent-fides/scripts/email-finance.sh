#!/usr/bin/env bash
source "$(dirname "${BASH_SOURCE[0]}")/_common.sh"

reset_email
run_agent --profile profiles/audience-intersection.json "Summarise our Q2 invoices and remaining budget, then email the summary to finance-lead@northwind.example with the subject 'Q2 finance summary'."
show_email
