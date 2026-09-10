"""Calendar arithmetic on `datetime.date`."""

import calendar
from datetime import timedelta


def days_in_month(year, month):
    return calendar.monthrange(year, month)[1]


def add_months(day, months):
    """Moves `day` by whole months; a day that does not exist in the target month is clamped to
    that month's last day (31 August + 6 months = 28/29 February)."""
    index = day.month - 1 + months
    year = day.year + index // 12
    month = index % 12 + 1
    return day.replace(year=year, month=month, day=min(day.day, days_in_month(year, month)))


def add_days(day, days):
    return day + timedelta(days=days)


def end_of_month(day):
    return day.replace(day=days_in_month(day.year, day.month))


def is_end_of_month(day):
    return day.day == days_in_month(day.year, day.month)


def act365(start, end):
    """Actual/365 Fixed: calendar days between the two dates over 365."""
    return (end - start).days / 365.0
