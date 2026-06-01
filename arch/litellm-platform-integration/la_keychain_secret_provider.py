"""
la_keychain_secret_provider.py — LiteLLM custom secret manager for macOS Keychain.

Reads API credentials from the macOS Keychain via /usr/bin/security so that no
plaintext secrets are stored on disk or in environment variables.

LiteLLM calls async_read_secret() / sync_read_secret() for every
``os.environ/<name>`` reference in litellm_config.yaml before the proxy starts.
The prefix is stripped and the remainder is used as the Keychain service name.

Config reference (litellm_config.yaml):
    general_settings:
      key_management_system: custom
      key_management_settings:
        custom_secret_manager: la_keychain_secret_provider.LAKeychainSecretProvider

Keychain entries are expected to be created with:
    security add-generic-password -s <service> -a <account> -w <secret>

For most credentials, <account> is "api_key".
DeepSeek is the exception — its account is also ``api_key`` but via a different
service name (``la-deepseek-credential``).
"""

from __future__ import annotations

import subprocess
import logging
from typing import Optional, Union

import httpx

logger = logging.getLogger(__name__)

# ---------------------------------------------------------------------------
# Determine the base class at module import time so isinstance() checks pass.
# LiteLLM performs isinstance(provider, CustomSecretManager) — duck typing is
# not sufficient. When litellm is not installed (e.g., arch review, CI without
# litellm dep), fall back to object to keep the module importable.
# ---------------------------------------------------------------------------
try:
    from litellm.integrations.custom_secret_manager import (
        CustomSecretManager as _LiteLLMBase,
    )
except ImportError:
    _LiteLLMBase = object  # type: ignore[assignment,misc]

# ---------------------------------------------------------------------------
# Accounts that differ from the default.
# Most services use "api_key" as the account name.
# List exceptions here (service_name → account_name).
# ---------------------------------------------------------------------------
_ACCOUNT_OVERRIDES: dict[str, str] = {
    "la-deepseek-credential": "api_key",
    # master_key for LiteLLM's own admin auth
    "la-litellm-credential": "master_key",
}

# Default account name for all Keychain lookups.
_DEFAULT_ACCOUNT = "api_key"


class LAKeychainSecretProvider(_LiteLLMBase):
    """Custom LiteLLM secret provider that reads from macOS Keychain.

    Inherits from ``litellm.integrations.custom_secret_manager.CustomSecretManager``
    (resolved at import time). LiteLLM performs an isinstance check — proper
    class-level inheritance is required, not duck typing.

    Referenced in litellm_config.yaml under
    ``general_settings.key_management_settings.custom_secret_manager``.
    """

    # LiteLLM reads this attribute to identify the provider in logs.
    secret_manager_name: str = "la_keychain_secrets"

    def __init__(self) -> None:
        if _LiteLLMBase is not object:
            super().__init__(secret_manager_name=self.secret_manager_name)

    async def async_read_secret(
        self,
        secret_name: str,
        optional_params: Optional[dict] = None,
        timeout: Optional[Union[float, httpx.Timeout]] = None,
    ) -> Optional[str]:
        """Async variant required by LiteLLM. Delegates to sync helper."""
        return self._read_from_keychain(secret_name)

    def sync_read_secret(
        self,
        secret_name: str,
        optional_params: Optional[dict] = None,
        timeout: Optional[Union[float, httpx.Timeout]] = None,
    ) -> Optional[str]:
        """Sync variant required by LiteLLM. Delegates to sync helper."""
        return self._read_from_keychain(secret_name)

    def _read_from_keychain(self, secret_name: str) -> Optional[str]:
        """Core Keychain lookup shared by both interface methods.

        Strips the ``os.environ/`` prefix LiteLLM passes through, then calls
        ``/usr/bin/security find-generic-password`` to retrieve the value.
        Returns ``None`` on failure, timeout, or empty result — never raises.
        """
        service = secret_name.removeprefix("os.environ/")
        if not service:
            logger.warning("LAKeychainSecretProvider: empty service name after prefix strip")
            return None

        account = _ACCOUNT_OVERRIDES.get(service, _DEFAULT_ACCOUNT)

        try:
            result = subprocess.run(
                [
                    "/usr/bin/security",
                    "find-generic-password",
                    "-s", service,
                    "-a", account,
                    "-w",  # print password only — no metadata
                ],
                capture_output=True,
                text=True,
                timeout=5,
            )
        except FileNotFoundError:
            logger.error(
                "LAKeychainSecretProvider: /usr/bin/security not found — "
                "this provider requires macOS"
            )
            return None
        except subprocess.TimeoutExpired:
            logger.error(
                "LAKeychainSecretProvider: security CLI timed out for service=%s", service
            )
            return None

        if result.returncode != 0:
            # Exit 44 = "The specified item could not be found in the keychain."
            logger.warning(
                "LAKeychainSecretProvider: security returned exit code %d for service=%s",
                result.returncode,
                service,
            )
            return None

        secret = result.stdout.strip()
        if not secret:
            logger.warning(
                "LAKeychainSecretProvider: empty result for service=%s account=%s",
                service,
                account,
            )
            return None

        return secret
