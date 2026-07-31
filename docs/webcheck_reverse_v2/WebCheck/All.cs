using System;
using System.Diagnostics;
using System.IO;
using System.Net;
using System.Text;
using System.Windows.Forms;
using Microsoft.VisualBasic;
using Microsoft.VisualBasic.CompilerServices;
using Microsoft.VisualBasic.FileIO;
using WebCheck.My;

namespace WebCheck;

[StandardModule]
internal sealed class All
{
	internal const string VersiyDLL = "6.0.8.1368";

	internal const int verAPIdefault = 2;

	internal const string DemoFn = "7000000512";

	internal const string MacTemp = "mmmaaaccc";

	internal const string IdTemp = "`";

	internal const string ZN = "0";

	internal const int LimitTime = 36;

	internal const int LimitTimeMoon = 168;

	internal const int DopuskLimitTime = 9;

	internal const string TestNameS = "_TS";

	internal static int Retries = 18;

	internal static int RetriesPrt = 81;

	internal static int Timing = 90000;

	internal const int TimingFree = 3000;

	internal const int TimingFull = 9000;

	internal const int TimingRobotA = 81000;

	internal const int TimingRobotL = 9000;

	internal const int SQLsave = 18;

	internal const int RetriesBlock = 108;

	internal const int SleepBlock = 500;

	internal const int MaxTimeBlockBase = 81;

	internal const int WithPrint57 = 29;

	internal const int WithPrint80 = 40;

	internal const int WithPrint57eco = 39;

	internal const int WithPrint80eco = 50;

	internal const int URLdefault = 1;

	internal const string URLtest = "cabinet.tax.gov.ua:9443";

	internal static string URLfact = "prro.tax.gov.ua:443";

	internal static string MacTempOld = "";

	internal static bool OfflineAllowed = true;

	internal static string OfflineNum = "";

	internal static string NumberTaxVk = "";

	internal const bool ProgramDataFolder = true;

	internal const string DotR = "#`#";

	internal const string XMLupStr = "<?xml version='1#`#0' encoding='WINDOWS-1251'?>";

	internal const string Proga = "WebCheck";

	internal static IniHGB f = new IniHGB(MyDoc() + "\\WebCheck\\settings.ini");

	internal static StringXML d = new StringXML();

	internal static SQLlite l = new SQLlite();

	internal static TypStart A;

	internal static TypStartCardserv B;

	internal static Directorys PayTax = new Directorys();

	internal static Reports Rf = new Reports();

	internal static Signature SF = new Signature();

	internal static LaunchСontrol lC = new LaunchСontrol();

	internal static string tMaxS = "";

	internal static LogSaveText Lg = new LogSaveText();

	internal static LogSaveText LgAll = new LogSaveText();

	internal static LogSaveText LgT = new LogSaveText();

	internal static IniHGB iT = new IniHGB(MyDoc() + "\\WebCheck\\Cardserv\\settings.ini");

	internal static IniHGB iTA = new IniHGB(PersonalTemp() + "pa.ini");

	private static string SRC = "";

	internal static TypNumFiscal InfaImport;

	internal static bool CardservTrue = false;

	internal static IniHGB ArS = new IniHGB(MyDoc() + "\\WebCheck\\Archive\\settings.ini");

	internal static string TinBux;

	internal const int PaySize = 180;

	internal const string PayUpTree = "├─";

	internal const string PayDownTree = "└─";

	internal const string PayUni = "•";

	internal static string PayU = "├─";

	internal static string PayD = "└─";

	internal const int PayNameMax = 18;

	internal const string PayTab = " ";

	internal const string TypPayCheck = "2";

	internal static bool Timer1Start = false;

	internal static bool SecondRunControl(bool SRCb = true)
	{
		if (A.Multiplayer)
		{
			return true;
		}
		if (!SRCb)
		{
			if (Operators.CompareString(SRC.Trim(), "", TextCompare: false) == 0)
			{
				DateTime now = DateTime.Now;
				string text = now.Hour.ToString() + now.Minute + now.Second;
				f.StringWriteFN(A.FN, "SRC", text.Trim());
				SRC = text.Trim();
				return true;
			}
			return false;
		}
		if (Operators.CompareString(SRC, f.StringGetFn(A.FN, "SRC").Trim(), TextCompare: false) == 0)
		{
			return true;
		}
		Lg.SaveTextToLog("Multiplayer=OFF", "Внимание! Опасная ситуация!", "Работают несколько экземпляров приложения.");
		return false;
	}

	internal static bool SecondRunControlR()
	{
		if (A.Multiplayer)
		{
			return true;
		}
		if (Operators.CompareString(SRC, f.StringGetFn(A.FN, "SRC").Trim(), TextCompare: false) == 0)
		{
			return true;
		}
		Lg.SaveTextToLog("Multiplayer=OFF", "Внимание! Опасная ситуация!", "Работают несколько экземпляров приложения.");
		return false;
	}

	internal static TypErrTimeShift ShiftTime()
	{
		TypErrTimeShift result = default(TypErrTimeShift);
		result.errCode = 0;
		result.errStr = "";
		result.returnShiftStart = "";
		result.returnShiftTime = false;
		result.returnTime = 0;
		string returnStr = l.ReturnOpenShiftTime().ReturnStr;
		if (Operators.CompareString(returnStr.Trim(), "", TextCompare: false) == 0)
		{
			result.errCode = 8;
			result.errStr = "Немає відкритої зміни.";
			result.returnShiftStart = "Немає відкритої зміни.";
			result.returnShiftTime = false;
			result.returnTime = 0;
			return result;
		}
		DateTime date = Convert.ToDateTime(returnStr);
		DateTime now = DateTime.Now;
		checked
		{
			int num = (int)DateAndTime.DateDiff(DateInterval.Minute, date, now);
			result.returnTime = 1440 - num;
			result.returnShiftStart = date.ToString();
			if (result.returnTime > 0)
			{
				result.returnShiftTime = true;
			}
			else
			{
				result.errCode = 46;
				result.errStr = "ПРРО заблоковано. Зміна була відкрита " + date.ToString() + " після чого минуло понад 24 години. Зніміть звіт Z.";
				result.returnShiftTime = false;
			}
			return result;
		}
	}

	internal static bool OfflineTimeOn()
	{
		if (DisableTimeControl())
		{
			return true;
		}
		if (Operators.CompareString(A.FN, "7000000512", TextCompare: false) == 0)
		{
			return true;
		}
		string text = l.OfflineDate().ReturnStr.Trim();
		if (Operators.CompareString(text, "", TextCompare: false) == 0)
		{
			return true;
		}
		DateTime date = Convert.ToDateTime(text);
		DateTime now = DateTime.Now;
		if (Math.Abs(DateAndTime.DateDiff(DateInterval.Minute, date, now)) > checked(TimeLimOffline() - 9))
		{
			return false;
		}
		return true;
	}

	private static bool DisableTimeControl()
	{
		if (Operators.CompareString(f.GetString("Global", "DTC"), "1", TextCompare: false) != 0)
		{
			return false;
		}
		return true;
	}

	internal static bool ControlSystemTime()
	{
		string text = l.MaxID("ksef").ReturnStr.Trim();
		if (Operators.CompareString(text, "0", TextCompare: false) == 0)
		{
			return true;
		}
		string text2 = l.CheckDate(text).ReturnStr.Trim();
		if (Operators.CompareString(text2, "", TextCompare: false) == 0)
		{
			return true;
		}
		DateTime date = Convert.ToDateTime(text2);
		DateTime now = DateTime.Now;
		if (DateAndTime.DateDiff(DateInterval.Second, date, now) < 0)
		{
			return false;
		}
		return true;
	}

	public static int TimeLimOffline()
	{
		int num = checked(10080 - f.IntegerGetFn(A.FN, "OfflineTime"));
		if (num < 2160)
		{
			return num;
		}
		return 2160;
	}

	internal static long DateOfflineClose()
	{
		DateTime dateTime = DateTime.Now;
		if (!OfflineTimeOn())
		{
			int num = checked(TimeLimOffline() - 3);
			if (num < 0)
			{
				num = 0;
			}
			DateTime dateValue = Convert.ToDateTime(l.OfflineDate().ReturnStr.Trim());
			dateTime = DateAndTime.DateAdd(DateInterval.Minute, num, dateValue);
		}
		return Convert.ToInt64(dateTime.Year + TwoSigns(dateTime.Month.ToString()) + TwoSigns(dateTime.Day.ToString()) + TwoSigns(dateTime.Hour.ToString()) + TwoSigns(dateTime.Minute.ToString()) + TwoSigns(dateTime.Second.ToString()));
	}

	internal static int RemTimeCurrentOffline()
	{
		string text = l.OfflineDate().ReturnStr.Trim();
		checked
		{
			if (Operators.CompareString(text, "", TextCompare: false) == 0)
			{
				return TimeLimOffline() - 9;
			}
			DateTime date = Convert.ToDateTime(text);
			DateTime now = DateTime.Now;
			return TimeLimOffline() - (int)Math.Abs(DateAndTime.DateDiff(DateInterval.Minute, date, now)) - 9;
		}
	}

	internal static string ReplacingQuotes(string txt)
	{
		txt = Strings.Replace(txt, "'", "\\'");
		txt = Strings.Replace(txt, "\"", "\\\"");
		return txt;
	}

	internal static TypErrStr ReplacingAMP(string XMLs)
	{
		TypErrStr result = default(TypErrStr);
		result.errCode = 0;
		result.errStr = "";
		result.ReturnStr = "";
		XMLs = Strings.Replace(XMLs, "&#34;", "```#34;");
		XMLs = Strings.Replace(XMLs, "&#38;", "```#38;");
		XMLs = Strings.Replace(XMLs, "&#39;", "```#39;");
		XMLs = Strings.Replace(XMLs, "&#60;", "```#60;");
		XMLs = Strings.Replace(XMLs, "&#62;", "```#62;");
		XMLs = Strings.Replace(XMLs, "&#46;", "```#46;");
		XMLs = Strings.Replace(XMLs, "&#8260;", "```#8260;");
		XMLs = Strings.Replace(XMLs, "&amp;#34;", "```#34;");
		XMLs = Strings.Replace(XMLs, "&amp;#38;", "```#38;");
		XMLs = Strings.Replace(XMLs, "&amp;#39;", "```#39;");
		XMLs = Strings.Replace(XMLs, "&amp;#60;", "```#60;");
		XMLs = Strings.Replace(XMLs, "&amp;#62;", "```#62;");
		XMLs = Strings.Replace(XMLs, "&amp;#46;", "```#46;");
		XMLs = Strings.Replace(XMLs, "&amp;#8260;", "```#8260;");
		XMLs = Strings.Replace(XMLs, "&amp;quot;", "```#34;");
		XMLs = Strings.Replace(XMLs, "&amp;&amp;", "```#38;");
		XMLs = Strings.Replace(XMLs, "&amp;&apos;", "```#39;");
		XMLs = Strings.Replace(XMLs, "&amp;&lt;", "```#60;");
		XMLs = Strings.Replace(XMLs, "&amp;&gt;", "```#62;");
		XMLs = Strings.Replace(XMLs, "&amp;&period;", "```#46;");
		XMLs = Strings.Replace(XMLs, "&amp;&frasl;", "```#8260;");
		XMLs = Strings.Replace(XMLs, "&quot;", "```#34;");
		XMLs = Strings.Replace(XMLs, "&amp;", "```#38;");
		XMLs = Strings.Replace(XMLs, "&apos;", "```#39;");
		XMLs = Strings.Replace(XMLs, "&lt;", "```#60;");
		XMLs = Strings.Replace(XMLs, "&gt;", "```#62;");
		XMLs = Strings.Replace(XMLs, "&period;", "```#46;");
		XMLs = Strings.Replace(XMLs, "&frasl;", "```#8260;");
		if (Zzz(XMLs, "&"))
		{
			result.errCode = 60;
			result.errStr = "В XML не правильно испольуется амперсанд";
			return result;
		}
		XMLs = Strings.Replace(XMLs, "```#34;", "&amp;#34;");
		XMLs = Strings.Replace(XMLs, "```#38;", "&amp;#38;");
		XMLs = Strings.Replace(XMLs, "```#39;", "&amp;#39;");
		XMLs = Strings.Replace(XMLs, "```#60;", "&amp;#60;");
		XMLs = Strings.Replace(XMLs, "```#62;", "&amp;#62;");
		XMLs = Strings.Replace(XMLs, "```#46;", "&amp;#46;");
		XMLs = Strings.Replace(XMLs, "```#8260;", "&amp;#8260;");
		result.ReturnStr = XMLs;
		return result;
	}

	internal static string HostToIP(string HostT)
	{
		string result;
		try
		{
			IPAddress[] hostAddresses = Dns.GetHostAddresses(HostT);
			result = hostAddresses[checked(hostAddresses.Length - 1)].ToString();
		}
		catch (Exception ex)
		{
			ProjectData.SetProjectError(ex);
			Exception ex2 = ex;
			result = "";
			ProjectData.ClearProjectError();
		}
		return result;
	}

	internal static double NalOld()
	{
		string returnStr = l.ReturnOpenShift().ReturnStr;
		if (!Versioned.IsNumeric(returnStr))
		{
			return 0.0;
		}
		if (Conversions.ToInteger(returnStr) < 1)
		{
			return 0.0;
		}
		PayTax.ZeroReport();
		Reports reports = new Reports();
		reports.Reprt2(returnStr);
		reports.Reprt4(returnStr);
		int payN = PayTax.PayN;
		int y = default(int);
		for (int i = 1; i <= payN; i = checked(i + 1))
		{
			if (Operators.CompareString(PayTax.get_ReportsPay(0, i).Trim(), "Готівка", TextCompare: false) == 0)
			{
				y = i;
				break;
			}
		}
		double num = StrToDouble(PayTax.get_ReportsPay(1, y));
		double num2 = StrToDouble(PayTax.get_ReportsPay(2, y));
		double num3 = StrToDouble(PayTax.SMI);
		double num4 = StrToDouble(PayTax.SMO);
		return StrToDouble(Bablo((num - num2 + (num3 - num4)).ToString()));
	}

	internal static double Nal()
	{
		string returnStr = l.ReturnOpenShift().ReturnStr;
		if (!Versioned.IsNumeric(returnStr))
		{
			return 0.0;
		}
		if (Conversions.ToInteger(returnStr) < 1)
		{
			return 0.0;
		}
		PayTax.ZeroReport();
		Reports reports = new Reports();
		reports.Reprt2(returnStr);
		reports.Reprt4(returnStr);
		string strD = reports.EPZtoCash(returnStr);
		int payN = PayTax.PayN;
		int y = default(int);
		for (int i = 1; i <= payN; i = checked(i + 1))
		{
			if (Operators.CompareString(PayTax.get_ReportsPay(0, i).Trim(), "Готівка", TextCompare: false) == 0)
			{
				y = i;
				break;
			}
		}
		double num = StrToDouble(PayTax.get_ReportsPay(1, y));
		double num2 = StrToDouble(PayTax.get_ReportsPay(2, y));
		double num3 = StrToDouble(PayTax.SMI);
		double num4 = StrToDouble(PayTax.SMO);
		double num5 = StrToDouble(strD);
		return StrToDouble(Bablo((num - num2 + (num3 - num4) - num5).ToString()));
	}

	internal static bool IsFullVersion()
	{
		KeysUpDate keysUpDate = new KeysUpDate();
		KeysWC keysWC = new KeysWC();
		DateTime now = DateTime.Now;
		string text = now.Year.ToString();
		string text2 = now.Month.ToString();
		if (text2.Length < 2)
		{
			text2 = "0" + text2;
		}
		text += text2;
		string text3 = now.Day.ToString();
		if (text3.Length < 2)
		{
			text3 = "0" + text3;
		}
		text += text3;
		bool result;
		try
		{
			long num = keysWC.DhgbK(A.TIN, A.FN);
			if (num < Conversions.ToInteger(text))
			{
				A.FullVersion = false;
				A.Fullend = "";
				goto IL_01a7;
			}
			string text4 = num.ToString();
			A.FullVersion = true;
			A.Fullend = Conversions.ToString(text4[6]) + Conversions.ToString(text4[7]) + "." + Conversions.ToString(text4[4]) + Conversions.ToString(text4[5]) + "." + Conversions.ToString(text4[0]) + Conversions.ToString(text4[1]) + Conversions.ToString(text4[2]) + Conversions.ToString(text4[3]);
			if (checked(num - Conversions.ToInteger(text)) <= 14)
			{
				goto IL_01a7;
			}
			result = true;
			goto end_IL_008e;
			IL_01a7:
			keysUpDate.DownloadKeyFile(A.TIN);
			Application.DoEvents();
			if (num < Conversions.ToInteger(text))
			{
				A.FullVersion = false;
				A.Fullend = "";
				result = false;
			}
			else
			{
				string text5 = num.ToString();
				A.FullVersion = true;
				A.Fullend = Conversions.ToString(text5[6]) + Conversions.ToString(text5[7]) + "." + Conversions.ToString(text5[4]) + Conversions.ToString(text5[5]) + "." + Conversions.ToString(text5[0]) + Conversions.ToString(text5[1]) + Conversions.ToString(text5[2]) + Conversions.ToString(text5[3]);
				result = true;
			}
			end_IL_008e:;
		}
		catch (Exception ex)
		{
			ProjectData.SetProjectError(ex);
			Exception ex2 = ex;
			A.FullVersion = false;
			A.Fullend = "";
			result = false;
			ProjectData.ClearProjectError();
		}
		return result;
	}

	internal static long СurrentCompDate()
	{
		DateTime now = DateTime.Now;
		return Convert.ToInt64(now.Year + TwoSigns(now.Month.ToString()) + TwoSigns(now.Day.ToString()) + TwoSigns(now.Hour.ToString()) + TwoSigns(now.Minute.ToString()) + TwoSigns(now.Second.ToString()));
	}

	private static string TwoSigns(string OneSigns)
	{
		if (OneSigns.Length < 2)
		{
			OneSigns = "0" + OneSigns;
		}
		return OneSigns;
	}

	internal static string VersionDll()
	{
		string result;
		try
		{
			result = FileVersionInfo.GetVersionInfo("C:\\Program Files\\WebCheck\\WebCheck\\WebCheck.dll").FileVersion;
		}
		catch (Exception ex)
		{
			ProjectData.SetProjectError(ex);
			Exception ex2 = ex;
			result = "6.0.8.1368";
			ProjectData.ClearProjectError();
		}
		return result;
	}

	internal static TypErrStr TestRegion()
	{
		TypErrStr result = default(TypErrStr);
		result.errCode = 0;
		result.errStr = "";
		result.ReturnStr = "";
		if (Versioned.IsNumeric("1.2"))
		{
			A.PointRegion = true;
			result.ReturnStr = "RegionSeparator=point";
		}
		else
		{
			A.PointRegion = false;
			result.ReturnStr = "RegionSeparator=comma";
		}
		return result;
	}

	internal static TypErr ConnectDB()
	{
		TypErr result = default(TypErr);
		result.errCode = 0;
		result.errStr = "";
		A.FileN = f.StringGetFn(A.FN, "Path");
		if (Operators.CompareString(A.FileN, "", TextCompare: false) == 0)
		{
			result.errCode = 1001;
			result.errStr = "Фискального Номера " + A.FN + " нет.";
		}
		else
		{
			A.Connection = "Data Source=" + A.FileN + "; Version=3";
			A.Status = true;
		}
		if (f.IntegerGetFn(A.FN, "FiscalMode") == 0)
		{
			A.FiscalMode = "cabinet.tax.gov.ua:9443";
		}
		else if (f.IntegerGetFn(A.FN, "FiscalMode") == 1)
		{
			A.FiscalMode = URLfact;
		}
		if (Operators.CompareString(f.StringGetFn(A.FN, "FiscalMode"), "", TextCompare: false) == 0)
		{
			A.FiscalMode = URLfact;
			f.StringWriteFN(A.FN, "FiscalMode", "1");
		}
		if (Operators.CompareString(A.FiscalMode, "cabinet.tax.gov.ua:9443", TextCompare: false) == 0)
		{
			if (File.Exists(Strings.Replace(A.FileN, A.FN, A.FN + "_TS")))
			{
				A.Connection = Strings.Replace(A.Connection, A.FN, A.FN + "_TS");
			}
			else
			{
				A.FiscalMode = URLfact;
			}
		}
		string text = f.StringGetFn(A.FN, "UseACSKTSPserver");
		if (Versioned.IsNumeric(text))
		{
			if (Conversions.ToInteger(text) > 0)
			{
				A.UseACSKTSPserver = true;
			}
			else
			{
				A.UseACSKTSPserver = false;
			}
		}
		else
		{
			A.UseACSKTSPserver = false;
			f.StringWriteFN(A.FN, "UseACSKTSPserver", "0");
		}
		A.AcskSettings = f.IntegerGetFn(A.FN, "Acsksettings");
		if (Operators.CompareString(f.StringGetFn(A.FN, "Acsksettings"), "", TextCompare: false) == 0)
		{
			f.StringWriteFN(A.FN, "Acsksettings", "0");
		}
		string text2 = f.StringGetFn(A.FN, "ShowPintForm");
		if (Versioned.IsNumeric(text2))
		{
			if (Conversions.ToInteger(text2) > 0)
			{
				A.ShowPrintForm = true;
			}
			else
			{
				A.ShowPrintForm = false;
			}
		}
		else
		{
			A.ShowPrintForm = true;
			f.StringWriteFN(A.FN, "ShowPintForm", "1");
		}
		string text3 = f.StringGetFn(A.FN, "EcoPrt");
		if (Versioned.IsNumeric(text3))
		{
			if (Conversions.ToInteger(text3) > 0)
			{
				A.ecoPrint = true;
			}
			else
			{
				A.ecoPrint = false;
			}
		}
		else
		{
			A.ecoPrint = false;
			f.StringWriteFN(A.FN, "EcoPrt", "0");
		}
		string text4 = f.StringGetFn(A.FN, "ShowPintFormX");
		if (Versioned.IsNumeric(text4))
		{
			if (Conversions.ToInteger(text4) > 0)
			{
				A.ShowPrintFormX = true;
			}
			else
			{
				A.ShowPrintFormX = false;
			}
		}
		else
		{
			A.ShowPrintFormX = true;
			f.StringWriteFN(A.FN, "ShowPintFormX", "1");
		}
		string text5 = f.StringGetFn(A.FN, "AutomatPrintCheck");
		if (Versioned.IsNumeric(text5))
		{
			if (Conversions.ToInteger(text5) > 0)
			{
				A.AutomatPrintCheck = true;
			}
			else
			{
				A.AutomatPrintCheck = false;
			}
		}
		else
		{
			A.AutomatPrintCheck = false;
			f.StringWriteFN(A.FN, "AutomatPrintCheck", "0");
		}
		A.PrinterName = f.StringGetFn(A.FN, "PrinterName");
		if (Operators.CompareString(f.StringGetFn(A.FN, "Offline"), "", TextCompare: false) == 0)
		{
			OfflineAllowed = true;
			f.StringWriteFN(A.FN, "Offline", "1");
		}
		else if (Operators.CompareString(f.StringGetFn(A.FN, "Offline"), "0", TextCompare: false) == 0)
		{
			OfflineAllowed = false;
		}
		else
		{
			OfflineAllowed = true;
		}
		OfflineAllowed = true;
		string text6 = f.StringGetFn(A.FN, "AutomatOfflineOn");
		if (Versioned.IsNumeric(text6))
		{
			if (Conversions.ToInteger(text6) != 2021)
			{
				A.AutomatOfflineOn = true;
			}
			else
			{
				A.AutomatOfflineOn = false;
			}
		}
		else
		{
			A.AutomatOfflineOn = true;
			f.StringWriteFN(A.FN, "AutomatOfflineOn", "1");
		}
		string text7 = f.StringGetFn(A.FN, "LogOn");
		if (Versioned.IsNumeric(text7))
		{
			if (Conversions.ToInteger(text7) > 0)
			{
				Lg.LogOn = true;
			}
			else
			{
				Lg.LogOn = false;
			}
		}
		else
		{
			Lg.LogOn = true;
			f.StringWriteFN(A.FN, "LogOn", "1");
		}
		if (!Versioned.IsNumeric(f.StringGetFn(A.FN, "OfflineMax")))
		{
			f.StringWriteFN(A.FN, "OfflineMax", "500");
		}
		if (!Versioned.IsNumeric(f.StringGetFn(A.FN, "OfflineMin")))
		{
			f.StringWriteFN(A.FN, "OfflineMin", "50");
		}
		int num = f.IntegerGetFn(A.FN, "OfflineTime");
		if (num > 10080)
		{
			f.IntigerWriteFN(A.FN, "OfflineTime", 10080);
		}
		else if (num < 1)
		{
			f.IntigerWriteFN(A.FN, "OfflineTime", 0);
		}
		string text8 = f.StringGetFn(A.FN, "ToPDF");
		if (Versioned.IsNumeric(text8))
		{
			if (Conversions.ToInteger(text8) > 0)
			{
				A.ToPDF = true;
			}
			else
			{
				A.ToPDF = false;
			}
		}
		else
		{
			A.ToPDF = false;
			f.StringWriteFN(A.FN, "ToPDF", "0");
		}
		text8 = f.StringGetFn(A.FN, "ToXML");
		if (Versioned.IsNumeric(text8))
		{
			if (Conversions.ToInteger(text8) > 0)
			{
				A.ToXML = true;
			}
			else
			{
				A.ToXML = false;
			}
		}
		else
		{
			A.ToXML = false;
			f.StringWriteFN(A.FN, "ToXML", "0");
		}
		text8 = f.StringGetFn(A.FN, "ToTXT");
		if (Versioned.IsNumeric(text8))
		{
			if (Conversions.ToInteger(text8) > 0)
			{
				A.ToTXT = true;
			}
			else
			{
				A.ToTXT = false;
			}
		}
		else
		{
			A.ToTXT = false;
			f.StringWriteFN(A.FN, "ToTXT", "0");
		}
		A.ExportLength = f.IntegerGetFn(A.FN, "ExportLength");
		if (A.ExportLength < 1)
		{
			A.ExportLength = 30;
			f.IntigerWriteFN(A.FN, "ExportLength", A.ExportLength);
		}
		if (A.ExportLength < 18)
		{
			A.ExportLength = 18;
			f.IntigerWriteFN(A.FN, "ExportLength", A.ExportLength);
		}
		if (f.IntegerGetFn(A.FN, "SendToEmail") > 0)
		{
			A.SendToEmail = true;
		}
		else
		{
			A.SendToEmail = false;
		}
		A.Delay = f.IntegerGetFn(A.FN, "Delay");
		if (A.Delay < 1)
		{
			A.Delay = 108;
			f.IntigerWriteFN(A.FN, "Delay", A.Delay);
		}
		A.LimitCertificate = f.IntegerGetFn(A.FN, "LimitCertificate");
		if (A.LimitCertificate < 1)
		{
			A.LimitCertificate = 2160;
			f.IntigerWriteFN(A.FN, "LimitCertificate", A.LimitCertificate);
		}
		string text9 = f.StringGetFn(A.FN, "Multiplayer");
		if (!Versioned.IsNumeric(text9.Trim()))
		{
			A.Multiplayer = true;
			f.StringWriteFN(A.FN, "Multiplayer", "1");
		}
		else if (Conversions.ToInteger(text9) > 0)
		{
			A.Multiplayer = true;
		}
		else
		{
			A.Multiplayer = false;
		}
		if (!Versioned.IsNumeric(f.StringGetFn(A.FN, "AllowableCash")))
		{
			f.IntigerWriteFN(A.FN, "AllowableCash", A.DopNal);
		}
		A.DopNal = f.IntegerGetFn(A.FN, "AllowableCash");
		if (A.DopNal < 0)
		{
			A.DopNal = 0;
		}
		if (A.DopNal > 50000)
		{
			A.DopNal = 50000;
		}
		if (Operators.CompareString(f.StringGetFn(A.FN, "Showacquiring"), "1", TextCompare: false) == 0)
		{
			A.Showacquiring = true;
		}
		else if (Operators.CompareString(f.StringGetFn(A.FN, "Showacquiring"), "", TextCompare: false) == 0)
		{
			A.Showacquiring = true;
			f.StringWriteFN(A.FN, "Showacquiring", "1");
		}
		else
		{
			A.Showacquiring = false;
			f.StringWriteFN(A.FN, "Showacquiring", "0");
		}
		if (f.IntegerGetFn(A.FN, "checkUID") > 0)
		{
			A.checkUID = true;
		}
		if (A.verAPI == 2 && DateTime.Now.Year < 2022 && DateTime.Now.Month < 10)
		{
			A.verAPI = 1;
		}
		int integer = f.GetInteger("Global", "apiver", 0);
		if (integer > 0)
		{
			A.verAPI = integer;
			if (A.verAPI > 2)
			{
				A.verAPI = 2;
			}
			if (A.verAPI < 1)
			{
				A.verAPI = 1;
			}
		}
		A.MonhtLast = f.IntegerGetFn(A.FN, "MonhtLast");
		if (A.MonhtLast < 1)
		{
			A.MonhtLast = 0;
			f.IntigerWriteFN(A.FN, "MonhtLast", A.MonhtLast);
		}
		A.DelTempCheck = f.IntegerGetFn(A.FN, "DelTempCheck");
		if (A.DelTempCheck < 1)
		{
			A.DelTempCheck = 3;
			f.IntigerWriteFN(A.FN, "DelTempCheck", A.DelTempCheck);
		}
		string @string = f.GetString("Global", "grpcproxy");
		if (@string.Length < 9)
		{
			f.WriteString("Global", "grpcproxy", "");
		}
		else
		{
			URLfact = @string;
		}
		A.ChekFooterSection = f.GetInteger("Global", "ChekFooterSection", 0);
		if (A.ChekFooterSection == 0)
		{
			f.WriteInteger("Global", "ChekFooterSection", -1);
			A.ChekFooterSection = -1;
		}
		if (f.GetInteger("Global", "offlinepullksefcheck", 0) == 1)
		{
			A.OfflinePullKsefCheck = true;
		}
		else
		{
			A.OfflinePullKsefCheck = false;
		}
		return result;
	}

	internal static void SettingsForServer()
	{
		if (A.TypWork == 2019)
		{
			A.SettingMode = true;
			A.AutomatOfflineOn = true;
			A.AutomatPrintCheck = false;
			A.ShowPrintForm = false;
			A.ShowPrintFormX = false;
			A.Multiplayer = true;
			SF.ErrorShow(ShowWindows: false);
		}
		else if (A.TypWork == 2020)
		{
			A.SettingMode = false;
			A.AutomatOfflineOn = true;
			A.AutomatPrintCheck = false;
			A.ShowPrintForm = false;
			A.ShowPrintFormX = false;
			A.Multiplayer = true;
			SF.ErrorShow(ShowWindows: false);
		}
		else if (A.TypWork == 2025)
		{
			A.SettingMode = true;
			A.AutomatOfflineOn = true;
			A.AutomatPrintCheck = false;
			A.ShowPrintForm = false;
			A.ShowPrintFormX = false;
			A.Multiplayer = true;
			SF.ErrorShow(ShowWindows: false);
		}
	}

	internal static string MyDoc()
	{
		string text = NewPath();
		if (Operators.CompareString(text, "", TextCompare: false) == 0)
		{
			return Environment.GetFolderPath(Environment.SpecialFolder.CommonApplicationData);
		}
		return text;
	}

	internal static string PersonalTemp()
	{
		return Path.GetTempPath();
	}

	internal static string PersonalDocT()
	{
		return Environment.GetFolderPath(Environment.SpecialFolder.Personal);
	}

	private static string NewPath()
	{
		string @string = new IniHGB(Environment.GetFolderPath(Environment.SpecialFolder.CommonApplicationData) + "\\WebCheck\\settings.ini").GetString("Global", "Path");
		if (@string.Trim().Length < 3)
		{
			return "";
		}
		return @string;
	}

	internal static void NewFolder()
	{
		if (!Directory.Exists(MyDoc() + "\\WebCheck\\"))
		{
			Directory.CreateDirectory(MyDoc() + "\\WebCheck\\");
		}
		if (!Directory.Exists(MyDoc() + "\\WebCheck\\DB\\"))
		{
			Directory.CreateDirectory(MyDoc() + "\\WebCheck\\DB\\");
		}
		if (!Directory.Exists(MyDoc() + "\\WebCheck\\Keys\\"))
		{
			Directory.CreateDirectory(MyDoc() + "\\WebCheck\\Keys\\");
		}
		if (!Directory.Exists(MyDoc() + "\\WebCheck\\Temp\\"))
		{
			Directory.CreateDirectory(MyDoc() + "\\WebCheck\\Temp\\");
		}
		if (!Directory.Exists(MyDoc() + "\\WebCheck\\Archive\\"))
		{
			Directory.CreateDirectory(MyDoc() + "\\WebCheck\\Archive\\");
		}
		if (!Directory.Exists(MyDoc() + "\\WebCheck\\Archive\\All\\"))
		{
			Directory.CreateDirectory(MyDoc() + "\\WebCheck\\Archive\\All\\");
		}
		if (!Directory.Exists(MyDoc() + "\\WebCheck\\Lic\\"))
		{
			Directory.CreateDirectory(MyDoc() + "\\WebCheck\\Lic\\");
		}
		if (!Directory.Exists(MyDoc() + "\\WebCheck\\Temp\\All\\"))
		{
			Directory.CreateDirectory(MyDoc() + "\\WebCheck\\Temp\\All\\");
		}
		if (!Directory.Exists(MyDoc() + "\\WebCheck\\Logo\\"))
		{
			Directory.CreateDirectory(MyDoc() + "\\WebCheck\\Logo\\");
		}
		if (!Directory.Exists(MyDoc() + "\\WebCheck\\Backup\\"))
		{
			Directory.CreateDirectory(MyDoc() + "\\WebCheck\\Backup\\");
		}
		if (!Directory.Exists(MyDoc() + "\\WebCheck\\Cardserv\\"))
		{
			Directory.CreateDirectory(MyDoc() + "\\WebCheck\\Cardserv\\");
		}
	}

	internal static void NewFolderFn()
	{
		string path = MyDoc() + "\\WebCheck\\Temp\\" + A.FN + "\\";
		if (!Directory.Exists(path))
		{
			Directory.CreateDirectory(path);
		}
		path = MyDoc() + "\\WebCheck\\Archive\\" + A.FN + "\\";
		if (!Directory.Exists(path))
		{
			Directory.CreateDirectory(path);
		}
	}

	internal static double TaxAmountr(double Amount, double TaxPr)
	{
		double num = 100.0;
		num = Amount / num;
		return TaxPr * num;
	}

	internal static double TaxAmountBig(double Amount, double TaxPr)
	{
		double num = 100.0 + TaxPr;
		num = Amount / num;
		return TaxPr * num;
	}

	internal static float StToSi(string strP)
	{
		float result;
		try
		{
			if (!A.PointRegion)
			{
				strP = Strings.Replace(strP, ".", ",");
				result = Conversions.ToSingle(strP);
			}
			else
			{
				strP = Strings.Replace(strP, ",", ".");
				result = Conversions.ToSingle(strP);
			}
		}
		catch (Exception ex)
		{
			ProjectData.SetProjectError(ex);
			Exception ex2 = ex;
			result = 0f;
			ProjectData.ClearProjectError();
		}
		return result;
	}

	internal static string SiToSt(float sinC)
	{
		string result;
		try
		{
			result = Strings.Replace(sinC.ToString(), ",", ".");
		}
		catch (Exception ex)
		{
			ProjectData.SetProjectError(ex);
			Exception ex2 = ex;
			result = "";
			ProjectData.ClearProjectError();
		}
		return result;
	}

	internal static double StrToDouble(string strD)
	{
		double result;
		try
		{
			string expression = strD;
			expression = (A.PointRegion ? Strings.Replace(expression, ",", ".") : Strings.Replace(expression, ".", ","));
			result = ((!Versioned.IsNumeric(expression)) ? 0.0 : Conversions.ToDouble(expression));
		}
		catch (Exception ex)
		{
			ProjectData.SetProjectError(ex);
			Exception ex2 = ex;
			result = 0.0;
			ProjectData.ClearProjectError();
		}
		return result;
	}

	internal static string NoSpaceString(string StrSpace)
	{
		string text = "";
		string text2 = "";
		checked
		{
			int num = StrSpace.Length - 1;
			for (int i = 0; i <= num; i++)
			{
				text2 = Conversions.ToString(StrSpace[i]);
				text += text2.Trim();
			}
			return text;
		}
	}

	internal static string Bablo(string sBablo, bool sDot = true)
	{
		string result;
		try
		{
			string expression = sBablo.Trim();
			expression = (A.PointRegion ? Strings.Replace(expression, ",", ".") : Strings.Replace(expression, ".", ","));
			if (Versioned.IsNumeric(expression))
			{
				expression = Strings.FormatNumber(expression, 9);
				expression = Strings.FormatNumber(expression, 8);
				expression = Strings.FormatNumber(expression, 7);
				expression = Strings.FormatNumber(expression, 6);
				expression = Strings.FormatNumber(expression, 5);
				expression = Strings.FormatNumber(expression, 4);
				expression = Strings.FormatNumber(expression, 3);
				expression = Strings.FormatNumber(expression, 2);
				expression = Strings.Replace(expression, ",", ".");
			}
			else
			{
				expression = "0.00";
			}
			expression = NoSpaceString(expression);
			result = ((!sDot) ? Strings.Replace(expression, ".", "") : expression);
		}
		catch (Exception ex)
		{
			ProjectData.SetProjectError(ex);
			Exception ex2 = ex;
			if (sDot)
			{
				result = "0.00";
				ProjectData.ClearProjectError();
			}
			else
			{
				result = "000";
				ProjectData.ClearProjectError();
			}
		}
		return result;
	}

	internal static string Bablo(float sBablo, bool sDot = true)
	{
		string expression = sBablo.ToString();
		expression = (A.PointRegion ? Strings.Replace(expression, ",", ".") : Strings.Replace(expression, ".", ","));
		if (Versioned.IsNumeric(expression))
		{
			expression = Strings.FormatNumber(expression, 9);
			expression = Strings.FormatNumber(expression, 8);
			expression = Strings.FormatNumber(expression, 7);
			expression = Strings.FormatNumber(expression, 6);
			expression = Strings.FormatNumber(expression, 5);
			expression = Strings.FormatNumber(expression, 4);
			expression = Strings.FormatNumber(expression, 3);
			expression = Strings.FormatNumber(expression, 2);
			expression = Strings.Replace(expression, ",", ".");
		}
		else
		{
			expression = "0.00";
		}
		expression = NoSpaceString(expression);
		if (sDot)
		{
			return expression;
		}
		return Strings.Replace(expression, ".", "");
	}

	internal static string Bablo(double sBablo, bool sDot = true)
	{
		string expression = sBablo.ToString();
		expression = (A.PointRegion ? Strings.Replace(expression, ",", ".") : Strings.Replace(expression, ".", ","));
		if (Versioned.IsNumeric(expression))
		{
			expression = Strings.FormatNumber(expression, 9);
			expression = Strings.FormatNumber(expression, 8);
			expression = Strings.FormatNumber(expression, 7);
			expression = Strings.FormatNumber(expression, 6);
			expression = Strings.FormatNumber(expression, 5);
			expression = Strings.FormatNumber(expression, 4);
			expression = Strings.FormatNumber(expression, 3);
			expression = Strings.FormatNumber(expression, 2);
			expression = Strings.Replace(expression, ",", ".");
		}
		else
		{
			expression = "0.00";
		}
		expression = NoSpaceString(expression);
		if (sDot)
		{
			return expression;
		}
		return Strings.Replace(expression, ".", "");
	}

	internal static string KolvoVes(string sKolvo, bool sDot = true)
	{
		string expression = sKolvo.Trim();
		expression = (A.PointRegion ? Strings.Replace(expression, ",", ".") : Strings.Replace(expression, ".", ","));
		if (Versioned.IsNumeric(expression))
		{
			expression = Strings.FormatNumber(expression, 3);
			expression = Strings.Replace(expression, ",", ".");
		}
		else
		{
			expression = "0.000";
		}
		expression = NoSpaceString(expression);
		if (sDot)
		{
			return expression;
		}
		return Strings.Replace(expression, ".", "");
	}

	internal static string NameNoDot(string NameS)
	{
		return Strings.Replace(NameS, ".", "#`#");
	}

	internal static bool InitializationNew(string strFN)
	{
		SF.SignatureStart();
		TypErrStr typErrStr = TestRegion();
		TypErrStr parametrToString = d.GetParametrToString(strFN, "fn");
		if (parametrToString.errCode > 0)
		{
			A.CurrentStatus = "Err=" + parametrToString.errCode;
			A.ErrHelp = parametrToString.errStr;
			A.ErrCode = parametrToString.errCode;
			return false;
		}
		if (!Versioned.IsNumeric(parametrToString.ReturnStr) | (parametrToString.ReturnStr.Length != 10))
		{
			parametrToString.errCode = 3;
			parametrToString.errStr = "Неправильний формат Фіскального Номера: " + parametrToString.ReturnStr;
			A.CurrentStatus = "Err=" + parametrToString.errCode;
			A.ErrHelp = parametrToString.errStr;
			A.ErrCode = parametrToString.errCode;
			return false;
		}
		f.ClearIndexIni();
		if (Operators.CompareString(f.StringGetFn(parametrToString.ReturnStr, "Path"), "", TextCompare: false) == 0)
		{
			parametrToString.errCode = 1001;
			parametrToString.errStr = "Фіскального номера " + parametrToString.ReturnStr + " у базі немає.";
			A.CurrentStatus = "Err=" + parametrToString.errCode;
			A.ErrCode = parametrToString.errCode;
			A.ErrHelp = parametrToString.errStr;
			return false;
		}
		if (f.IntegerGetFn(parametrToString.ReturnStr, "On") < 1)
		{
			parametrToString.errCode = 1;
			parametrToString.errStr = "Доступ до Фіскального Номеру " + parametrToString.ReturnStr + " заборонено.";
			A.CurrentStatus = "Err=" + parametrToString.errCode;
			A.ErrHelp = parametrToString.errStr;
			A.ErrCode = parametrToString.errCode;
			return false;
		}
		if (A.Status)
		{
			parametrToString.errCode = 2;
			parametrToString.errStr = "Раніше було підключено Фіскальний Номер: " + parametrToString.ReturnStr;
			A.CurrentStatus = "Err=" + parametrToString.errCode;
			A.ErrHelp = parametrToString.errStr;
			A.ErrCode = parametrToString.errCode;
			return false;
		}
		A.FN = parametrToString.ReturnStr.Trim();
		TypErr typErr = ConnectDB();
		if (typErr.errCode > 0)
		{
			A.CurrentStatus = "Err=" + parametrToString.errCode;
			A.ErrHelp = typErr.errStr;
			A.ErrCode = typErr.errCode;
			return false;
		}
		typErr = PayTax.StartLoad();
		if (typErr.errCode > 0)
		{
			A.CurrentStatus = "Err=" + typErr.errCode;
			A.ErrHelp = typErr.errStr;
			A.ErrCode = typErr.errCode;
			return false;
		}
		IsFullVersion();
		string text = "";
		text = ((!A.FullVersion) ? "free" : A.Fullend);
		A.CurrentStatus = "Err=0_TIN=" + A.TIN + "_FN=" + A.FN + "_" + typErrStr.ReturnStr + "_version=" + VersionDll() + "_license=" + text;
		A.ErrHelp = "";
		A.ErrCode = 0;
		return true;
	}

	internal static bool FinalizationNew()
	{
		f.ClearIndexIni();
		A.Status = false;
		return true;
	}

	internal static string UTFtoANSI(string strUTF)
	{
		Encoding encoding = Encoding.GetEncoding("KOI8-RU");
		Encoding uTF = Encoding.UTF8;
		byte[] bytes = uTF.GetBytes(strUTF);
		byte[] bytes2 = Encoding.Convert(uTF, encoding, bytes);
		return encoding.GetString(bytes2);
	}

	internal static bool XMLtoTextFile(string fileOld, string fileNew)
	{
		bool result;
		try
		{
			StreamReader streamReader = new StreamReader(fileOld, Encoding.UTF8);
			string s = streamReader.ReadToEnd();
			byte[] bytes = Encoding.Default.GetBytes(s);
			s = Encoding.UTF8.GetString(bytes);
			StreamWriter streamWriter = new StreamWriter(fileNew, append: false, Encoding.GetEncoding(1251));
			streamWriter.Write(s);
			streamReader.Dispose();
			streamWriter.Dispose();
			result = true;
		}
		catch (Exception ex)
		{
			ProjectData.SetProjectError(ex);
			Exception ex2 = ex;
			result = false;
			ProjectData.ClearProjectError();
		}
		return result;
	}

	public static bool SaveToFileText(string FileNameNow, string TextSave)
	{
		try
		{
			if (File.Exists(FileNameNow))
			{
				Microsoft.VisualBasic.FileIO.FileSystem.DeleteFile(FileNameNow);
			}
			Application.DoEvents();
			if (File.Exists(FileNameNow + ".p7s"))
			{
				Microsoft.VisualBasic.FileIO.FileSystem.DeleteFile(FileNameNow + ".p7s");
			}
			Application.DoEvents();
		}
		catch (Exception ex)
		{
			ProjectData.SetProjectError(ex);
			Exception ex2 = ex;
			ProjectData.ClearProjectError();
		}
		bool result;
		try
		{
			Stream stream = File.OpenWrite(FileNameNow);
			StreamWriter streamWriter = new StreamWriter(stream, Encoding.GetEncoding(1251));
			streamWriter.Write(TextSave);
			Application.DoEvents();
			streamWriter.Flush();
			streamWriter.Close();
			result = true;
		}
		catch (Exception ex3)
		{
			ProjectData.SetProjectError(ex3);
			Exception ex4 = ex3;
			result = false;
			ProjectData.ClearProjectError();
		}
		return result;
	}

	public static TypErrStr PreviousMAC(string NumberShift, bool operINN = false)
	{
		TypErrStr result = default(TypErrStr);
		result.errCode = 0;
		result.errStr = "";
		result.ReturnStr = "";
		TypErrMacNumDat typErrMacNumDat = default(TypErrMacNumDat);
		typErrMacNumDat.returnStr = "";
		typErrMacNumDat.errStr = "";
		typErrMacNumDat.errCode = 0;
		typErrMacNumDat.returnNumber = "";
		typErrMacNumDat.returnData = "";
		typErrMacNumDat = ReturnLastCheckMac(NumberShift, operINN);
		result.ReturnStr = typErrMacNumDat.returnStr;
		result.errStr = typErrMacNumDat.errStr;
		result.errCode = typErrMacNumDat.errCode;
		return result;
	}

	public static TypErrStr SubstitutePreviousMAC(string StrXML, string NumberShift, bool operINN = false)
	{
		TypErrStr result = default(TypErrStr);
		result.errCode = 0;
		result.errStr = "";
		result.ReturnStr = "";
		if (Operators.CompareString(A.FN, "7000000512", TextCompare: false) == 0)
		{
			StrXML = StrXML.Replace("mmmaaaccc", "<MAC>ВебЧеК тестовый ПРРО</MAC>");
			result.ReturnStr = StrXML;
			return result;
		}
		if (l.OfflineTrue())
		{
			StrXML = StrXML.Replace("mmmaaaccc", "<MAC>mmmaaaccc</MAC>");
			result.ReturnStr = StrXML;
			return result;
		}
		TypErrMacNumDat typErrMacNumDat = default(TypErrMacNumDat);
		typErrMacNumDat.returnStr = "";
		typErrMacNumDat.errStr = "";
		typErrMacNumDat.errCode = 0;
		typErrMacNumDat.returnNumber = "";
		typErrMacNumDat.returnData = "";
		if (Operators.CompareString(MacTempOld, "", TextCompare: false) == 0)
		{
			typErrMacNumDat = ReturnLastCheckMac(NumberShift, operINN);
			if (typErrMacNumDat.errCode > 0)
			{
				MacTempOld = "error";
				result.ReturnStr = typErrMacNumDat.returnStr;
				result.errStr = typErrMacNumDat.errStr;
				result.errCode = typErrMacNumDat.errCode;
				return result;
			}
			MacTempOld = typErrMacNumDat.returnStr;
		}
		if (Operators.CompareString(MacTempOld, "error", TextCompare: false) == 0)
		{
			typErrMacNumDat.returnStr = StrXML;
			typErrMacNumDat.errCode = 32;
			typErrMacNumDat.errStr = "Переход в офлайн режим";
		}
		else
		{
			typErrMacNumDat.returnStr = "<MAC>" + MacTempOld + "</MAC>";
			StrXML = StrXML.Replace("mmmaaaccc", typErrMacNumDat.returnStr);
			typErrMacNumDat.returnStr = StrXML;
		}
		result.ReturnStr = typErrMacNumDat.returnStr;
		result.errStr = typErrMacNumDat.errStr;
		result.errCode = typErrMacNumDat.errCode;
		return result;
	}

	internal static TypErrMacNumDat ReturnLastCheckMac(string NumberShift, bool operINN = false)
	{
		TypErrMacNumDat result = default(TypErrMacNumDat);
		result.errCode = 0;
		result.errStr = "";
		result.returnStr = "";
		result.returnNumber = "";
		result.returnData = "";
		SaveToFileTextFN();
		TypErrOperKeyPass typErrOperKeyPass;
		if (operINN)
		{
			typErrOperKeyPass = l.OperatorInfa(NumberShift);
			if (typErrOperKeyPass.errCode > 0)
			{
				result.errCode = typErrOperKeyPass.errCode;
				result.errStr = typErrOperKeyPass.errStr;
				return result;
			}
		}
		else
		{
			typErrOperKeyPass = l.OperatorKeyPass(NumberShift);
			if (typErrOperKeyPass.errCode > 0)
			{
				result.errCode = typErrOperKeyPass.errCode;
				result.errStr = typErrOperKeyPass.errStr;
				return result;
			}
		}
		string text = MyDoc() + "\\WebCheck\\Temp\\" + A.FN + "\\FN.xml";
		TypErr typErr = SF.SignatureFile(typErrOperKeyPass.KeyFile.Trim(), typErrOperKeyPass.Pass.Trim(), text);
		if (typErr.errCode > 0)
		{
			result.errCode = typErr.errCode;
			result.errStr = typErr.errStr;
			return result;
		}
		text += ".p7s";
		SubmitPtr submitPtr = default(SubmitPtr);
		TypErrSubmit typErrSubmit = submitPtr.LastCheck(text);
		if (typErrSubmit.errCode > 0)
		{
			result.errCode = typErrSubmit.errCode;
			result.errStr = typErrSubmit.errStr + "  Status: " + typErrSubmit.returnStatus + "  Msg: " + typErrSubmit.returnStr + "   ";
			return result;
		}
		result.returnNumber = typErrSubmit.returnNumber;
		result.returnData = typErrSubmit.returnData;
		if (Operators.CompareString(typErrSubmit.returnData.Trim(), "", TextCompare: false) == 0)
		{
			result.returnStr = "";
			return result;
		}
		string filePathString = MyDoc() + "\\WebCheck\\Temp\\" + A.FN + "\\000.xml";
		SF.DecodCheck(typErrSubmit.returnData);
		if (result.errCode > 0)
		{
			result.returnStr = "";
			return result;
		}
		Application.DoEvents();
		result.returnStr = SHA.GenerateSHA256File(filePathString);
		return result;
	}

	private static bool SaveToFileTextFN()
	{
		string text = MyDoc() + "\\WebCheck\\Temp\\" + A.FN + "\\FN.xml";
		string fN = A.FN;
		try
		{
			if (File.Exists(text))
			{
				Microsoft.VisualBasic.FileIO.FileSystem.DeleteFile(text);
				Application.DoEvents();
			}
			if (File.Exists(text + ".p7s"))
			{
				Microsoft.VisualBasic.FileIO.FileSystem.DeleteFile(text + ".p7s");
				Application.DoEvents();
			}
		}
		catch (Exception ex)
		{
			ProjectData.SetProjectError(ex);
			Exception ex2 = ex;
			ProjectData.ClearProjectError();
		}
		bool result;
		try
		{
			Stream stream = File.OpenWrite(text);
			StreamWriter streamWriter = new StreamWriter(stream, Encoding.GetEncoding(1251));
			streamWriter.Write(fN);
			Application.DoEvents();
			streamWriter.Flush();
			streamWriter.Close();
			result = true;
		}
		catch (Exception ex3)
		{
			ProjectData.SetProjectError(ex3);
			Exception ex4 = ex3;
			result = false;
			ProjectData.ClearProjectError();
		}
		return result;
	}

	internal static string CheckNumToTyp(string numT)
	{
		return numT switch
		{
			"0" => "продаж", 
			"1" => "повернення", 
			"3" => "внесення", 
			"4" => "видача", 
			"8" => "відкриття", 
			"9" => "офлайн", 
			"10" => "онлайн", 
			"12" => "запит", 
			"80" => "Z звіт", 
			"-8" => "видача епз", 
			_ => "помилка", 
		};
	}

	internal static TypErr ForbiddenSymbols(string strXML)
	{
		TypErr result = default(TypErr);
		result.errCode = 0;
		result.errStr = "";
		strXML = strXML.Trim();
		if (Operators.CompareString(strXML, "", TextCompare: false) == 0)
		{
			result.errCode = 60;
			result.errStr = "Помилка! Не може бути порожнім рядком";
			return result;
		}
		strXML = strXML.ToLower();
		string text = "<>'\"";
		checked
		{
			int num = text.Length - 1;
			for (int i = 0; i <= num; i++)
			{
				if (Zzz(strXML, Conversions.ToString(text[i])))
				{
					result.errCode = 60;
					result.errStr = "Помилка! У XML використовується заборонений символ";
					return result;
				}
			}
			return result;
		}
	}

	private static bool Zzz(string strXml, string strP)
	{
		if (Strings.InStr(strXml, strP) > 0)
		{
			return true;
		}
		return false;
	}

	internal static string DelTabooS(string S)
	{
		string text = "_=";
		string text2 = S;
		checked
		{
			int num = text.Length - 1;
			for (int i = 0; i <= num; i++)
			{
				if (Zzz(S, Conversions.ToString(text[i])))
				{
					text2 = Strings.Replace(text2, Conversions.ToString(text[i]), " ");
				}
			}
			return text2;
		}
	}

	internal static string TwoS(string OneS)
	{
		OneS = OneS.Trim();
		if (OneS.Length == 1)
		{
			OneS = "0" + OneS;
		}
		return OneS;
	}

	internal static TypProductName DecoderProductName(string NameS)
	{
		TypProductName result = default(TypProductName);
		result.Name = "";
		result.Uktzed = "";
		result.Excisestamp = "";
		int num = 1;
		int length = NameS.Length;
		checked
		{
			int num2 = length - 1;
			for (int i = 0; i <= num2; i++)
			{
				string text = Conversions.ToString(NameS[i]);
				string text2 = "";
				string text3 = "";
				if (i + 2 <= length - 1)
				{
					text2 = Conversions.ToString(NameS[i + 1]);
					text3 = Conversions.ToString(NameS[i + 2]);
				}
				if (Operators.CompareString(text + text2 + text3, "~`~", TextCompare: false) == 0)
				{
					num++;
					i += 2;
					continue;
				}
				switch (num)
				{
				case 1:
					result.Name += text;
					break;
				case 2:
					result.Uktzed += text;
					break;
				case 3:
					result.Excisestamp += text;
					break;
				}
			}
			return result;
		}
	}

	internal static bool CertificateTrue(string NumCertificate)
	{
		return true;
	}

	internal static void DelIniDat()
	{
		int num = f.IndexMaxFn();
		for (int i = 1; i <= num; i = checked(i + 1))
		{
			string text = f.NameFn(i);
			if (Operators.CompareString(text, "del", TextCompare: false) != 0 && Operators.CompareString(text.Trim(), "", TextCompare: false) != 0)
			{
				string text2 = MyDoc() + "\\WebCheck\\Temp\\" + text + "\\dat.ini";
				if (File.Exists(text2))
				{
					Microsoft.VisualBasic.FileIO.FileSystem.DeleteFile(text2);
				}
			}
		}
	}

	internal static TypErr OfflineOn()
	{
		if (A.AutomatOfflineOn)
		{
			return OfflineOnManually();
		}
		TypErr result = default(TypErr);
		result.errStr = "Помилка зв'язку із сервером АЦСК (TSP).";
		result.errCode = 44;
		return result;
	}

	internal static TypErr OfflineOnManually()
	{
		TypErr result = default(TypErr);
		result.errStr = "";
		result.errCode = 0;
		OfflineNum = "";
		TypErrStr typErrStr = new NumbersOfflineUse().OfflineID();
		if (typErrStr.errCode > 0)
		{
			result.errCode = typErrStr.errCode;
			result.errStr = typErrStr.errStr;
			return result;
		}
		OfflineNum = typErrStr.ReturnStr;
		string text = "109";
		if (A.verAPI == 1)
		{
			text = "9";
		}
		long num = СurrentCompDate();
		int num2 = checked(Conversions.ToInteger(l.MaxID("ksef").ReturnStr) + 1);
		string text2 = "<RQ V ='1'>";
		text2 = text2 + "<DAT  FN='" + A.FN + "'";
		text2 = text2 + " TN='ПН " + A.INN + "' ZN=''";
		text2 = text2 + " DI='" + num2 + "' V='1'>";
		text2 = text2 + "<C T='" + text + "'></C>";
		text2 = text2 + "<TS>" + num + "</TS></DAT>";
		text2 += "mmmaaaccc</RQ>";
		if (Conversions.ToInteger(l.MaxID("ksef").ReturnStr) < 1)
		{
			result.errCode = 89;
			result.errStr = "Помилка. Увімкнення офлайн режиму неможливо. Для початку роботи з ПРРО перший документ має бути прийнятий сервером податкової в режимі онлайн.";
			return result;
		}
		TypErr typErr = l.SaveXMLcheckOffline("Service", text2, text2, "not", typErrStr.ReturnStr, "9");
		if (typErr.errCode > 0)
		{
			result.errCode = typErr.errCode;
			result.errStr = typErr.errStr;
			return result;
		}
		return result;
	}

	internal static TypErr OfflineOnTechno()
	{
		TypErr result = default(TypErr);
		result.errStr = "";
		result.errCode = 0;
		OfflineNum = "";
		string checkidficscal = (OfflineNum = "ТехнічнийЧек");
		string text = "109";
		if (A.verAPI == 1)
		{
			text = "9";
		}
		long num = СurrentCompDate();
		int num2 = checked(Conversions.ToInteger(l.MaxID("ksef").ReturnStr) + 1);
		string text2 = "<RQ V ='1'>";
		text2 = text2 + "<DAT  FN='" + A.FN + "'";
		text2 = text2 + " TN='ПН " + A.INN + "' ZN=''";
		text2 = text2 + " DI='" + num2 + "' V='1'>";
		text2 = text2 + "<C T='" + text + "'></C>";
		text2 = text2 + "<TS>" + num + "</TS></DAT>";
		text2 += "mmmaaaccc</RQ>";
		TypErr typErr = l.SaveXMLcheckOfflineTechno("Service", text2, text2, "not", checkidficscal, "9");
		if (typErr.errCode > 0)
		{
			result.errCode = typErr.errCode;
			result.errStr = typErr.errStr;
			return result;
		}
		return result;
	}

	internal static bool AccessAWS()
	{
		bool result;
		try
		{
			string address = "http://s3.eu-west-2.amazonaws.com/che.ck.ua/s3.txt";
			string text = PersonalTemp() + "s3.txt";
			if (File.Exists(text))
			{
				Microsoft.VisualBasic.FileIO.FileSystem.DeleteFile(text);
			}
			MyProject.Computer.Network.DownloadFile(address, text);
			result = true;
		}
		catch (Exception ex)
		{
			ProjectData.SetProjectError(ex);
			Exception ex2 = ex;
			Lg.SaveTextToLog("AccessAWS", ex2.Message);
			result = false;
			ProjectData.ClearProjectError();
		}
		return result;
	}

	internal static string MethodEnToUa(string e)
	{
		return e switch
		{
			"purchase" => "Продаж", 
			"refund" => "Повернення", 
			"cashback" => "Видача  ЕПЗ", 
			"verify" => "Загальний звiт", 
			"audit" => "X-звіт", 
			"identify" => "Перевірка", 
			"getmerchantlist" => "Список Мерчантів", 
			"withdrawal" => "Відміна", 
			"ping" => "Пінг", 
			_ => "", 
		};
	}

	internal static bool Money(string m)
	{
		m = m.Trim();
		if (m.Length == 0)
		{
			return false;
		}
		if ((m.Length == 1) & (Operators.CompareString(Conversions.ToString(m[0]), "0", TextCompare: false) == 0))
		{
			return false;
		}
		if ((m.Length == 2) & (Operators.CompareString(Conversions.ToString(m[0]), "00", TextCompare: false) == 0))
		{
			return false;
		}
		if ((m.Length == 3) & (Operators.CompareString(Conversions.ToString(m[0]), "000", TextCompare: false) == 0))
		{
			return false;
		}
		if (Operators.CompareString(Conversions.ToString(m[0]), "-", TextCompare: false) == 0)
		{
			return false;
		}
		return true;
	}

	internal static bool KeyReplacement(string innO)
	{
		if (l.OfflineTrue())
		{
			return false;
		}
		IniHGB iniHGB = new IniHGB(MyDoc() + "\\WebCheck\\Temp\\" + A.FN + "\\dat.ini");
		if (Operators.CompareString(iniHGB.GetString("R" + innO, "Updated"), "", TextCompare: false) == 0)
		{
			return false;
		}
		UpdateKeys updateKeys = new UpdateKeys();
		if (!updateKeys.UpdateKey(innO))
		{
			return false;
		}
		if (!updateKeys.DelKey(innO))
		{
			return false;
		}
		iniHGB.WriteString(innO, "EndKey", "");
		iniHGB.WriteString(innO, "Updated", "");
		return true;
	}

	internal static void DelFileControl(string e)
	{
		try
		{
			if (File.Exists(e))
			{
				File.Delete(e);
			}
		}
		catch (Exception ex)
		{
			ProjectData.SetProjectError(ex);
			Exception ex2 = ex;
			ProjectData.ClearProjectError();
		}
	}
}
