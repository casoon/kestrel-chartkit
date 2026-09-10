"""Business days and the conventions that move a date off a non-business day."""

from .dates import add_days

UNADJUSTED = "unadjusted"
FOLLOWING = "following"
MODIFIED_FOLLOWING = "modified_following"
PRECEDING = "preceding"

#: In the order the fixtures number them.
CONVENTIONS = (UNADJUSTED, FOLLOWING, MODIFIED_FOLLOWING, PRECEDING)


class BusinessCalendar:
    """Saturdays, Sundays and an explicit holiday list — nothing else."""

    def __init__(self, holidays):
        self.holidays = frozenset(holidays)

    def is_business_day(self, day):
        return day.isoweekday() <= 5 and day not in self.holidays

    def _roll(self, day, step):
        while not self.is_business_day(day):
            day = add_days(day, step)
        return day

    def adjust(self, day, convention):
        """Following moves forward to the next business day, Preceding backward; Modified
        Following moves forward unless that leaves the month, and then backward instead."""
        if convention == UNADJUSTED:
            return day
        if convention == FOLLOWING:
            return self._roll(day, 1)
        if convention == PRECEDING:
            return self._roll(day, -1)
        if convention == MODIFIED_FOLLOWING:
            forward = self._roll(day, 1)
            if (forward.year, forward.month) == (day.year, day.month):
                return forward
            return self._roll(day, -1)
        raise ValueError(f"unknown convention {convention!r}")
