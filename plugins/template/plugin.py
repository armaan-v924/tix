from __future__ import annotations


def main(context, argv):
    print(f"plugin={context.plugin_name}")
    print(f"ticket_root={context.ticket_root}")
    print(f"argv={argv}")

    ticket = context.ticket or {}
    print(f"ticket_id={ticket.get('id')}")
