using System.IO;
using System.Windows.Forms;
using Microsoft.VisualBasic;
using Microsoft.VisualBasic.FileIO;

namespace WebCheck;

internal class DelFN
{
	internal bool DeleteFN(string FN = "7000000512")
	{
		if (All.A.Status)
		{
			return false;
		}
		string text = All.f.StringGetFn(FN, "Path");
		if (File.Exists(text))
		{
			Microsoft.VisualBasic.FileIO.FileSystem.DeleteFile(text);
			Application.DoEvents();
		}
		text = Strings.Replace(text, FN, FN + "_TS");
		if (File.Exists(text))
		{
			Microsoft.VisualBasic.FileIO.FileSystem.DeleteFile(text);
			Application.DoEvents();
		}
		All.f.StringWriteFN(FN, "Path", "");
		All.f.StringWriteFN(FN, "TIN", "");
		All.f.StringWriteFN(FN, "On", "");
		All.f.StringWriteFN(FN, "Save", "");
		All.f.StringWriteFN(FN, "ShowPintForm", "");
		All.f.StringWriteFN(FN, "LogOn", "");
		All.f.StringWriteFN(FN, "UseACSKTSPserver", "");
		All.f.StringWriteFN(FN, "ShowPintFormX", "");
		All.f.StringWriteFN(FN, "AutomatPrintCheck", "");
		All.f.StringWriteFN(FN, "Offline", "");
		All.f.StringWriteFN(FN, "OfflineMax", "");
		All.f.StringWriteFN(FN, "OfflineMin", "");
		All.f.StringWriteFN(FN, "OfflineTime", "");
		All.f.StringWriteFN(FN, "FiscalMode", "");
		All.f.StringWriteFN(FN, "Acsksettings", "");
		All.f.StringWriteFN(FN, "PrinterName", "");
		All.f.StringWriteFN(FN, "ToPDF", "");
		All.f.StringWriteFN(FN, "ToXML", "");
		All.f.StringWriteFN(FN, "ToTXT", "");
		All.f.StringWriteFN(FN, "ExportLength", "");
		All.f.StringWriteFN(FN, "Block", "");
		All.f.StringWriteFN(FN, "AutomatOfflineOn", "");
		All.f.StringWriteFN(FN, "IndicatorY", "");
		All.f.StringWriteFN(FN, "IndicatorStepY", "");
		All.f.StringWriteFN(FN, "Delay", "");
		All.f.StringWriteFN(FN, "eMail", "");
		All.f.StringWriteFN(FN, "Pass", "");
		All.f.StringWriteFN(FN, "Port", "");
		All.f.StringWriteFN(FN, "HostSMTP", "");
		All.f.StringWriteFN(FN, "TimeOut", "");
		All.f.StringWriteFN(FN, "EnableSSL", "");
		All.f.StringWriteFN(FN, "UseDefaultCredentials", "");
		All.f.StringWriteFN(FN, "SendToEmail", "");
		All.f.StringWriteFN(FN, "IndicatorVisible", "");
		All.f.StringWriteFN(FN, "PrinterWidth", "");
		All.f.DelFn(FN);
		return true;
	}
}
