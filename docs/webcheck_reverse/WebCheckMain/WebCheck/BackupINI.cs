using System;

namespace WebCheck;

internal class BackupINI
{
	private string PathINI;

	private IniHGB bINI;

	public BackupINI()
	{
		PathINI = All.MyDoc() + "\\WebCheck\\Backup\\" + All.A.FN + ".ini";
		bINI = new IniHGB(PathINI);
	}

	internal void StartBackup()
	{
		if (All.A.FullVersion)
		{
			bINI.WriteString("Upload", "Z", "9");
			bINI.WriteString("Upload", "LastOrder", DateTime.Now.ToString());
			CopyINI();
		}
		else
		{
			bINI.WriteString("Upload", "Z", "0");
			bINI.WriteString("Upload", "LastOrder", "Only available in full version");
		}
	}

	private void CopyINI()
	{
		string section = "Backup";
		string value = All.f.StringGetFn(All.A.FN, "Path");
		bINI.WriteString(section, "Path", value);
		value = All.f.StringGetFn(All.A.FN, "TIN");
		bINI.WriteString(section, "TIN", value);
		value = All.f.StringGetFn(All.A.FN, "On");
		bINI.WriteString(section, "On", value);
		value = All.f.StringGetFn(All.A.FN, "Save");
		bINI.WriteString(section, "Save", value);
		value = All.f.StringGetFn(All.A.FN, "ShowPintForm");
		bINI.WriteString(section, "ShowPintForm", value);
		value = All.f.StringGetFn(All.A.FN, "LogOn");
		bINI.WriteString(section, "LogOn", value);
		value = All.f.StringGetFn(All.A.FN, "FiscalMode");
		bINI.WriteString(section, "FiscalMode", value);
		value = All.f.StringGetFn(All.A.FN, "UseACSKTSPserver");
		bINI.WriteString(section, "UseACSKTSPserver", value);
		value = All.f.StringGetFn(All.A.FN, "Acsksettings");
		bINI.WriteString(section, "Acsksettings", value);
		value = All.f.StringGetFn(All.A.FN, "EcoPrt");
		bINI.WriteString(section, "EcoPrt", value);
		value = All.f.StringGetFn(All.A.FN, "ShowPintFormX");
		bINI.WriteString(section, "ShowPintFormX", value);
		value = All.f.StringGetFn(All.A.FN, "AutomatPrintCheck");
		bINI.WriteString(section, "AutomatPrintCheck", value);
		value = All.f.StringGetFn(All.A.FN, "Offline");
		bINI.WriteString(section, "Offline", value);
		value = All.f.StringGetFn(All.A.FN, "AutomatOfflineOn");
		bINI.WriteString(section, "AutomatOfflineOn", value);
		value = All.f.StringGetFn(All.A.FN, "OfflineMax");
		bINI.WriteString(section, "OfflineMax", value);
		value = All.f.StringGetFn(All.A.FN, "OfflineMin");
		bINI.WriteString(section, "OfflineMin", value);
		value = All.f.StringGetFn(All.A.FN, "OfflineTime");
		bINI.WriteString(section, "OfflineTime", value);
		value = All.f.StringGetFn(All.A.FN, "ToPDF");
		bINI.WriteString(section, "ToPDF", value);
		value = All.f.StringGetFn(All.A.FN, "ToXML");
		bINI.WriteString(section, "ToXML", value);
		value = All.f.StringGetFn(All.A.FN, "ToTXT");
		bINI.WriteString(section, "ToTXT", value);
		value = All.f.StringGetFn(All.A.FN, "ExportLength");
		bINI.WriteString(section, "ExportLength", value);
		value = All.f.StringGetFn(All.A.FN, "Delay");
		bINI.WriteString(section, "Delay", value);
		value = All.f.StringGetFn(All.A.FN, "LimitCertificate");
		bINI.WriteString(section, "LimitCertificate", value);
		value = All.f.StringGetFn(All.A.FN, "Multiplayer");
		bINI.WriteString(section, "Multiplayer", value);
		value = All.f.StringGetFn(All.A.FN, "AllowableCash");
		bINI.WriteString(section, "AllowableCash", value);
		value = All.f.StringGetFn(All.A.FN, "Showacquiring");
		bINI.WriteString(section, "Showacquiring", value);
		value = All.f.StringGetFn(All.A.FN, "MonhtLast");
		bINI.WriteString(section, "MonhtLast", value);
		value = All.f.StringGetFn(All.A.FN, "DelTempCheck");
		bINI.WriteString(section, "DelTempCheck", value);
		value = All.f.StringGetFn(All.A.FN, "ShowInTaskbar");
		bINI.WriteString(section, "ShowInTaskbar", value);
		value = All.f.StringGetFn(All.A.FN, "IndicatorVisible");
		bINI.WriteString(section, "IndicatorVisible", value);
		value = All.f.StringGetFn(All.A.FN, "IndicatorY");
		bINI.WriteString(section, "IndicatorY", value);
		value = All.f.StringGetFn(All.A.FN, "IndicatorStepY");
		bINI.WriteString(section, "IndicatorStepY", value);
		value = All.f.StringGetFn(All.A.FN, "PrinterWidth");
		bINI.WriteString(section, "PrinterWidth", value);
	}
}
