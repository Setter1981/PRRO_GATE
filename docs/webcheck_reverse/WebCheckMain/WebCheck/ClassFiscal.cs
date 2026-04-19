using System;
using System.ComponentModel;
using System.IO;
using System.Threading;
using System.Windows.Forms;
using Microsoft.VisualBasic;
using Microsoft.VisualBasic.CompilerServices;
using Microsoft.VisualBasic.FileIO;

namespace WebCheck;

public class ClassFiscal
{
	public CheckStamp CheckLine;

	public ForServer ServerSetGet;

	public ClassFiscal()
	{
		CheckLine = new CheckStamp();
		ServerSetGet = new ForServer();
		All.NewFolder();
		All.f.WriteString("Global", "WWW", "https://www.webchek.com.ua/");
		All.A.FN = "";
		All.A.FileN = "";
		All.A.Status = false;
		All.A.Connection = "";
		All.A.CurrentStatus = "";
		All.A.ErrHelp = "";
		All.A.ErrCode = 0;
		All.A.Report = "";
		All.A.FullVersion = false;
		All.A.Fullend = "";
		All.A.TIN = "";
		All.A.OrgName = "";
		All.A.PointAddr = "";
		All.A.PointName = "";
		All.A.INN = "";
		All.A.OperatorINN = "";
		All.A.PassKey = "";
		All.A.PathKey = "";
		All.A.ShowPrintForm = false;
		All.A.ShowPrintFormX = true;
		All.A.AcskSettings = 0;
		All.A.AcskSettingsTemp = 0;
		All.A.AutomatPrintCheck = false;
		All.A.UseACSKTSPserver = false;
		All.A.PrinterName = "";
		All.A.ToPDF = false;
		All.A.ToTXT = false;
		All.A.ToPDF = false;
		All.A.ExportLength = 0;
		All.A.AutomatOfflineOn = false;
		All.A.Delay = 1;
		All.A.TestInt = 0;
		All.A.SendToEmail = false;
		All.A.LimitCertificate = 2160;
		All.A.Multiplayer = true;
		All.A.DopNal = 50000;
		All.A.TypWork = 0;
		All.A.FormRobot = false;
		All.A.PullY = 0;
		All.A.Showacquiring = true;
		All.A.checkUID = false;
		All.A.RoundingCash = false;
		All.A.RoundingCashINI = false;
		All.A.FiscalMode = All.URLfact;
		All.A.SettingMode = false;
		All.A.verAPI = 2;
		All.A.ecoPrint = false;
		All.A.DelTempCheck = 0;
		All.A.MonhtLast = 0;
		All.A.CheckTimeLog = false;
		All.A.ChekFooterSection = -1;
		All.A.OfflinePullKsefCheck = false;
		All.SF.ErrorShow(ShowWindows: false);
		All.SF.ErrorShow(ShowWindows: false);
	}

	~ClassFiscal()
	{
		try
		{
			All.l.BlockBase(bl: false);
			All.lC.StopControl();
		}
		catch (Exception ex)
		{
			ProjectData.SetProjectError(ex);
			Exception ex2 = ex;
			ProjectData.ClearProjectError();
		}
		base.Finalize();
	}

	public bool Initialization(string strFN)
	{
		int year = DateTime.Now.Year;
		All.LgAll.PathFile = All.MyDoc() + "\\WebCheck\\Temp\\log_" + year + ".txt";
		TypErrStr typErrStr = All.TestRegion();
		TypErrStr parametrToString = All.d.GetParametrToString(strFN, "fn");
		if (parametrToString.errCode > 0)
		{
			All.A.CurrentStatus = "Err=" + parametrToString.errCode;
			All.A.ErrHelp = parametrToString.errStr;
			All.A.ErrCode = parametrToString.errCode;
			return false;
		}
		if (!Versioned.IsNumeric((object)parametrToString.ReturnStr) | (parametrToString.ReturnStr.Length != 10))
		{
			parametrToString.errCode = 3;
			parametrToString.errStr = "Неправильний формат Фіскального Номера: " + parametrToString.ReturnStr;
			All.A.CurrentStatus = "Err=" + parametrToString.errCode;
			All.A.ErrHelp = parametrToString.errStr;
			All.A.ErrCode = parametrToString.errCode;
			return false;
		}
		All.f.ClearIndexIni();
		if (Operators.CompareString(All.f.StringGetFn(parametrToString.ReturnStr, "Path"), "", false) == 0)
		{
			parametrToString.errCode = 1001;
			parametrToString.errStr = "Фіскального номера " + parametrToString.ReturnStr + " у базі немає.";
			All.A.CurrentStatus = "Err=" + parametrToString.errCode;
			All.A.ErrCode = parametrToString.errCode;
			All.A.ErrHelp = parametrToString.errStr;
			return false;
		}
		if (All.A.Status)
		{
			parametrToString.errCode = 2;
			parametrToString.errStr = "Раніше було підключено Фіскальний Номер: " + All.A.FN;
			All.A.CurrentStatus = "Err=" + parametrToString.errCode;
			All.A.ErrHelp = parametrToString.errStr;
			All.A.ErrCode = parametrToString.errCode;
			return false;
		}
		All.A.FN = parametrToString.ReturnStr.Trim();
		All.NewFolderFn();
		All.Lg.PathFile = All.MyDoc() + "\\WebCheck\\Temp\\" + All.A.FN + "\\log.txt";
		TypErr typErr = All.ConnectDB();
		if (typErr.errCode > 0)
		{
			All.A.CurrentStatus = "Err=" + parametrToString.errCode;
			All.A.ErrHelp = typErr.errStr;
			All.A.ErrCode = typErr.errCode;
			All.A.Status = false;
			All.Lg.SaveTextToLog("Initialization", strFN, StatusBarLog());
			return false;
		}
		if (All.Lg.LastLog())
		{
			All.Lg.SaveTextToLog("Initialization", "ВНИМАНИЕ!", "Выполнена процедура архивации и очистки лог-файла");
		}
		All.A.SettingMode = false;
		All.SettingsForServer();
		All.SF.ErrorShow(ShowWindows: false);
		CreateDB createDB = new CreateDB(All.A.FN);
		if (File.Exists(All.MyDoc() + "\\WebCheck\\Backup\\" + All.A.FN + ".db") && !All.l.TableBackuplog())
		{
			createDB.CreateTable(13);
			createDB.CreateTrigerBackup();
		}
		createDB.CreateSessions(newPRO: false);
		createDB.CreateIndex(newPRO: false);
		parametrToString = All.d.GetParametrToString(strFN, "mode");
		if (parametrToString.errCode == 0 && Versioned.IsNumeric((object)parametrToString.ReturnStr) && Conversions.ToInteger(parametrToString.ReturnStr) == 1)
		{
			All.A.SettingMode = true;
		}
		typErr = All.PayTax.StartLoad();
		if (typErr.errCode > 0)
		{
			All.A.CurrentStatus = "Err=" + typErr.errCode;
			All.A.ErrHelp = typErr.errStr;
			All.A.ErrCode = typErr.errCode;
			All.A.Status = false;
			All.Lg.SaveTextToLog("Initialization", strFN, StatusBarLog());
			return false;
		}
		All.SF.SignatureStart();
		All.IsFullVersion();
		if (Operators.CompareString(All.A.FN, "7000000512", false) == 0)
		{
			All.A.FullVersion = true;
		}
		string text;
		if (All.A.FullVersion)
		{
			text = All.A.Fullend;
			All.Retries = 3;
			All.Timing = 9000;
		}
		else
		{
			text = "free";
			All.Retries = 9;
			All.Timing = 3000;
		}
		if (!All.A.SettingMode && All.A.AutomatOfflineOn)
		{
			All.lC.StartControlForm(All.A.FN);
		}
		All.SecondRunControl(SRCb: false);
		if (!All.ControlSystemTime())
		{
			All.A.ErrHelp = "Увага! Системний час менше за час на останньому чеку.";
			All.A.ErrCode = 101;
			All.A.CurrentStatus = "Err=" + All.A.ErrCode;
			All.Lg.SaveTextToLog("Initialization", "Ошибка системного времени", "Системное время меньше времени на поледнем чеке");
			return false;
		}
		if (((All.A.TypWork < 2019) | (All.A.TypWork > 2020)) && !All.A.AutomatOfflineOn)
		{
			Offlin offlin = new Offlin();
			All.l.OpenShiftReturn = false;
			offlin.NewBackupNumbers();
			All.l.OpenShiftReturn = true;
		}
		NumbersOfflineUse numbersOfflineUse = new NumbersOfflineUse();
		All.A.CurrentStatus = "Err=0_TIN=" + All.A.TIN + "_FN=" + All.A.FN + "_" + typErrStr.ReturnStr + "_version=" + All.VersionDll() + "_license=" + text + "_OfflineCount=" + All.l.OfflineCheckCount() + "_Offline=" + All.l.OfflineTrueInt() + "_OfflinePool=" + numbersOfflineUse.CountNubmers() + "_ApiVer=" + All.A.verAPI;
		All.A.ErrHelp = "";
		All.A.ErrCode = 0;
		All.Lg.SaveTextToLog("Initialization", strFN, StatusBarLog());
		return true;
	}

	public bool Finalization(string strFN)
	{
		TypErrStr parametrToString = All.d.GetParametrToString(strFN, "fn");
		if (parametrToString.errCode > 0)
		{
			All.A.CurrentStatus = "Err=" + parametrToString.errCode;
			All.A.ErrHelp = parametrToString.errStr;
			All.A.ErrCode = parametrToString.errCode;
			All.Lg.SaveTextToLog("Finalization", strFN, StatusBarLog());
			return false;
		}
		if (!Versioned.IsNumeric((object)parametrToString.ReturnStr) | (parametrToString.ReturnStr.Length != 10))
		{
			parametrToString.errCode = 3;
			parametrToString.errStr = "Неправильний формат Фіскального Номера: " + parametrToString.ReturnStr;
			All.A.CurrentStatus = "Err=" + parametrToString.errCode;
			All.A.ErrHelp = parametrToString.errStr;
			All.A.ErrCode = parametrToString.errCode;
			All.Lg.SaveTextToLog("Finalization", strFN, StatusBarLog());
			return false;
		}
		if (All.A.TypWork != 2025 && Operators.CompareString(All.f.StringGetFn(parametrToString.ReturnStr, "Path"), "", false) == 0)
		{
			parametrToString.errCode = 1001;
			parametrToString.errStr = "Фіскального номера " + parametrToString.ReturnStr + " у базі немає.";
			All.A.CurrentStatus = "Err=" + parametrToString.errCode;
			All.A.ErrCode = parametrToString.errCode;
			All.A.ErrHelp = parametrToString.errStr;
			All.Lg.SaveTextToLog("Finalization", strFN, StatusBarLog());
			return false;
		}
		All.f.ClearIndexIni();
		if (!All.A.Status)
		{
			parametrToString.errCode = 4;
			parametrToString.errStr = "Нет подключения.";
			All.A.CurrentStatus = "Err=" + parametrToString.errCode;
			All.A.ErrHelp = parametrToString.errStr;
			All.A.ErrCode = parametrToString.errCode;
			All.Lg.SaveTextToLog("Finalization", strFN, StatusBarLog());
			return false;
		}
		if (Operators.CompareString(parametrToString.ReturnStr, All.A.FN, false) == 0)
		{
			All.A.SettingMode = false;
			All.A.Status = false;
			All.lC.StopControl();
			All.A.CurrentStatus = "Err=0_FN=" + All.A.FN;
			All.A.ErrHelp = "";
			All.A.ErrCode = 0;
			All.Lg.SaveTextToLog("Finalization", strFN, StatusBarLog());
			return true;
		}
		parametrToString.errCode = 2;
		parametrToString.errStr = "Підєднано інший Фіскальний Номер";
		All.A.CurrentStatus = "Err=" + parametrToString.errCode;
		All.A.ErrHelp = parametrToString.errStr;
		All.A.ErrCode = parametrToString.errCode;
		All.Lg.SaveTextToLog("Finalization", strFN, StatusBarLog());
		return false;
	}

	public bool OpenShift(string strXML)
	{
		TypErrStr parametrToString = All.d.GetParametrToString(strXML, "fn");
		if (parametrToString.errCode > 0)
		{
			All.A.CurrentStatus = "Err=" + parametrToString.errCode;
			All.A.ErrHelp = parametrToString.errStr;
			All.A.ErrCode = parametrToString.errCode;
			All.Lg.SaveTextToLog("OpenShift", strXML, StatusBarLog());
			return false;
		}
		if (!Versioned.IsNumeric((object)parametrToString.ReturnStr) | (parametrToString.ReturnStr.Length != 10))
		{
			parametrToString.errCode = 3;
			parametrToString.errStr = "Неправильний формат Фіскального Номера: " + parametrToString.ReturnStr;
			All.A.CurrentStatus = "Err=" + parametrToString.errCode;
			All.A.ErrHelp = parametrToString.errStr;
			All.A.ErrCode = parametrToString.errCode;
			All.Lg.SaveTextToLog("OpenShift", strXML, StatusBarLog());
			return false;
		}
		if (Operators.CompareString(All.f.StringGetFn(parametrToString.ReturnStr, "Path"), "", false) == 0)
		{
			parametrToString.errCode = 1001;
			parametrToString.errStr = "Фіскального номера " + parametrToString.ReturnStr + " у базі немає.";
			All.A.CurrentStatus = "Err=" + parametrToString.errCode;
			All.A.ErrCode = parametrToString.errCode;
			All.A.ErrHelp = parametrToString.errStr;
			All.Lg.SaveTextToLog("OpenShift", strXML, StatusBarLog());
			return false;
		}
		if (All.f.IntegerGetFn(parametrToString.ReturnStr, "On") < 1)
		{
			parametrToString.errCode = 1;
			parametrToString.errStr = "Доступ к Фискальному Номеру " + parametrToString.ReturnStr + " запрещен.";
			All.A.CurrentStatus = "Err=" + parametrToString.errCode;
			All.A.ErrHelp = parametrToString.errStr;
			All.A.ErrCode = parametrToString.errCode;
			All.Lg.SaveTextToLog("OpenShift", strXML, StatusBarLog());
			return false;
		}
		if (All.A.Status)
		{
			if (Operators.CompareString(parametrToString.ReturnStr, All.A.FN, false) != 0)
			{
				parametrToString.errCode = 2;
				parametrToString.errStr = "Ранее был подключен другой ФН.";
				All.A.CurrentStatus = "Err=" + parametrToString.errCode;
				All.A.ErrHelp = parametrToString.errStr;
				All.A.ErrCode = parametrToString.errCode;
				All.Lg.SaveTextToLog("OpenShift", strXML, StatusBarLog());
				return false;
			}
			parametrToString = All.d.GetParametrToString(strXML, "OperatorID");
			if (parametrToString.errCode > 0)
			{
				All.A.CurrentStatus = "Err=" + parametrToString.errCode;
				All.A.ErrHelp = parametrToString.errStr;
				All.A.ErrCode = parametrToString.errCode;
				All.Lg.SaveTextToLog("OpenShift", strXML, StatusBarLog());
				return false;
			}
			string returnStr = parametrToString.ReturnStr;
			parametrToString = All.l.OperatorName(returnStr);
			if (parametrToString.errCode > 0)
			{
				All.A.CurrentStatus = "Err=" + parametrToString.errCode;
				All.A.ErrHelp = parametrToString.errStr;
				All.A.ErrCode = parametrToString.errCode;
				All.Lg.SaveTextToLog("OpenShift", strXML, StatusBarLog());
				return false;
			}
			string returnStr2 = parametrToString.ReturnStr;
			parametrToString = All.l.ReturnOpenShift();
			if (parametrToString.errCode > 0)
			{
				All.A.CurrentStatus = "Err=" + parametrToString.errCode;
				All.A.ErrHelp = parametrToString.errStr;
				All.A.ErrCode = parametrToString.errCode;
				All.Lg.SaveTextToLog("OpenShift", strXML, StatusBarLog());
				return false;
			}
			if (Conversions.ToInteger(parametrToString.ReturnStr) > 0)
			{
				parametrToString.errCode = 1006;
				parametrToString.errStr = "Вже є відкрита зміна.";
				All.A.CurrentStatus = "Err=" + parametrToString.errCode;
				All.A.ErrHelp = parametrToString.errStr;
				All.A.ErrCode = parametrToString.errCode;
				All.Lg.SaveTextToLog("OpenShift", strXML, StatusBarLog());
				return false;
			}
			if (!All.SF.KeyShiftTimeINN(returnStr))
			{
				if (All.KeyReplacement(returnStr))
				{
					All.A.CurrentStatus = "Err=" + 66;
					All.A.ErrHelp = "Увага! У поточного ключа закінчується термін дії, він успішно замінений на відкладений ключ. Можете продовжити роботу.";
					All.A.ErrCode = 66;
					All.Lg.SaveTextToLog("OpenShift", strXML, StatusBarLog());
					return false;
				}
				All.A.CurrentStatus = "Err=" + 66;
				All.A.ErrHelp = "Помилка. Сертифікат цього оператора закінчується або про нього немає інформації. Перевірте термін дії вашої ЕЦП (для перевірки виконайте вхід кабінету платника податків).";
				All.A.ErrCode = 66;
				All.Lg.SaveTextToLog("OpenShift", strXML, StatusBarLog());
				return false;
			}
			if (!All.OfflineTimeOn())
			{
				parametrToString.errStr = "Перевищено ліміт часу роботи в режимі офлайн (" + All.f.StringGetFn(All.A.FN, "LastOfflineErr") + ")";
				parametrToString.errCode = 42;
				All.A.CurrentStatus = "Err=" + parametrToString.errCode;
				All.A.ErrHelp = parametrToString.errStr;
				All.A.ErrCode = parametrToString.errCode;
				All.Lg.SaveTextToLog("OpenShift", strXML, StatusBarLog());
				return false;
			}
			IniHGB iniHGB = new IniHGB(All.MyDoc() + "\\WebCheck\\Temp\\" + All.A.FN + "\\bl.ini");
			int num = 0;
			while (iniHGB.IntegerGetFn(All.A.FN, "Block") != 0)
			{
				Application.DoEvents();
				Thread.Sleep(500);
				num = checked(num + 1);
				if (num > 108)
				{
					break;
				}
			}
			if (iniHGB.IntegerGetFn(All.A.FN, "Block") == 1)
			{
				parametrToString.errStr = "База заблокированна, ведется отправка офлайн чеков.";
				parametrToString.errCode = 1015;
				All.A.CurrentStatus = "Err=" + parametrToString.errCode;
				All.A.ErrHelp = parametrToString.errStr;
				All.A.ErrCode = parametrToString.errCode;
				All.Lg.SaveTextToLog("OpenShift", strXML, StatusBarLog());
				return false;
			}
			if (!new ControlTime().TimeContol(27))
			{
				parametrToString.errStr = "Системний час відрізняється від податкового сервера.";
				parametrToString.errCode = 77;
				All.A.CurrentStatus = "Err=" + parametrToString.errCode;
				All.A.ErrHelp = parametrToString.errStr;
				All.A.ErrCode = parametrToString.errCode;
				All.Lg.SaveTextToLog("OpenShift", strXML, StatusBarLog());
				return false;
			}
			if (Conversions.ToInteger(All.l.MaxID("ksef").ReturnStr) > 1)
			{
				OfflineOnC();
			}
			if (!All.l.BlockBase(bl: true))
			{
				All.A.CurrentStatus = "Err=" + 57;
				All.A.ErrHelp = "Превышен лимит времени ожидания доступа к базе";
				All.A.ErrCode = 57;
				All.Lg.SaveTextToLog("OpenShift", strXML, StatusBarLog());
				return false;
			}
			All.A.OperatorINN = returnStr;
			TypErr typErr = All.d.OpenShiftXML(returnStr, returnStr2);
			All.l.BlockBase(bl: false);
			if (typErr.errCode == 32)
			{
				if (!All.l.BlockBase(bl: true))
				{
					All.A.CurrentStatus = "Err=" + 57;
					All.A.ErrHelp = "Превышен лимит времени ожидания доступа к базе";
					All.A.ErrCode = 57;
					All.Lg.SaveTextToLog("OpenShift", strXML, StatusBarLog());
					return false;
				}
				typErr = All.d.OpenShiftXML(returnStr, returnStr2);
				All.l.BlockBase(bl: false);
			}
			if (typErr.errCode > 0)
			{
				All.A.CurrentStatus = "Err=" + typErr.errCode;
				All.A.ErrHelp = typErr.errStr;
				All.A.ErrCode = typErr.errCode;
				All.Lg.SaveTextToLog("OpenShift", strXML, StatusBarLog());
				return false;
			}
			All.l.TransferBackup();
			string returnStr3 = All.l.MaxID("ksef").ReturnStr;
			string strXML2 = "<InputParameters><Parameters ID='" + returnStr3 + "'/></InputParameters>";
			if (All.A.ShowPrintForm)
			{
				ShowPrintByCheckFn(strXML2);
			}
			TypPrintChecks typPrintChecks = All.Rf.CheckXMLNumber(returnStr3, SearchID: true);
			new PrintExportCheck().ExportToFile(typPrintChecks.ReturnStr, typPrintChecks.ReturnStrTaxN);
			TypErrStr typErrStr = All.l.ReturnOpenShift();
			if (typErrStr.errCode > 0)
			{
				All.A.CurrentStatus = "Err=" + typErrStr.errCode;
				All.A.ErrHelp = typErrStr.errStr;
				All.A.ErrCode = typErrStr.errCode;
				All.Lg.SaveTextToLog("OpenShift", strXML, StatusBarLog());
				return false;
			}
			All.A.CurrentStatus = "Err=0_FN=" + All.A.FN + "_ShiftNumber=" + typErrStr.ReturnStr + "_OperatorName=" + returnStr2;
			All.A.ErrHelp = "";
			All.A.ErrCode = 0;
			All.Lg.SaveTextToLog("OpenShift", strXML, StatusBarLog());
			return true;
		}
		parametrToString.errCode = 4;
		parametrToString.errStr = "Нет подключения";
		All.A.CurrentStatus = "Err=" + parametrToString.errCode;
		All.A.ErrHelp = parametrToString.errStr;
		All.A.ErrCode = parametrToString.errCode;
		All.Lg.SaveTextToLog("OpenShift", strXML, StatusBarLog());
		return false;
	}

	public bool ReportZ(string strXML)
	{
		DateTime now = DateTime.Now;
		All.SecondRunControl();
		TypErrStr parametrToString = All.d.GetParametrToString(strXML, "fn");
		if (parametrToString.errCode > 0)
		{
			All.A.CurrentStatus = "Err=" + parametrToString.errCode;
			All.A.ErrHelp = parametrToString.errStr;
			All.A.ErrCode = parametrToString.errCode;
			All.Lg.SaveTextToLog("ReportZ", strXML, StatusBarLog());
			return false;
		}
		if (!Versioned.IsNumeric((object)parametrToString.ReturnStr) | (parametrToString.ReturnStr.Length != 10))
		{
			parametrToString.errCode = 3;
			parametrToString.errStr = "ННеправильний формат Фіскального Номера: " + parametrToString.ReturnStr;
			All.A.CurrentStatus = "Err=" + parametrToString.errCode;
			All.A.ErrHelp = parametrToString.errStr;
			All.A.ErrCode = parametrToString.errCode;
			All.Lg.SaveTextToLog("ReportZ", strXML, StatusBarLog());
			return false;
		}
		if (Operators.CompareString(All.f.StringGetFn(parametrToString.ReturnStr, "Path"), "", false) == 0)
		{
			parametrToString.errCode = 1001;
			parametrToString.errStr = "Фіскального номера " + parametrToString.ReturnStr + " у базі немає.";
			All.A.CurrentStatus = "Err=" + parametrToString.errCode;
			All.A.ErrCode = parametrToString.errCode;
			All.A.ErrHelp = parametrToString.errStr;
			All.Lg.SaveTextToLog("ReportZ", strXML, StatusBarLog());
			return false;
		}
		if (All.f.IntegerGetFn(parametrToString.ReturnStr, "On") < 1)
		{
			parametrToString.errCode = 1;
			parametrToString.errStr = "Доступ к Фискальному Номеру " + parametrToString.ReturnStr + " запрещен.";
			All.A.CurrentStatus = "Err=" + parametrToString.errCode;
			All.A.ErrHelp = parametrToString.errStr;
			All.A.ErrCode = parametrToString.errCode;
			All.Lg.SaveTextToLog("ReportZ", strXML, StatusBarLog());
			return false;
		}
		if (All.A.Status)
		{
			if (Operators.CompareString(parametrToString.ReturnStr, All.A.FN, false) != 0)
			{
				parametrToString.errCode = 2;
				parametrToString.errStr = "Ранее был подключен другой ФН.";
				All.A.CurrentStatus = "Err=" + parametrToString.errCode;
				All.A.ErrHelp = parametrToString.errStr;
				All.A.ErrCode = parametrToString.errCode;
				All.Lg.SaveTextToLog("ReportZ", strXML, StatusBarLog());
				return false;
			}
			string returnStr = parametrToString.ReturnStr;
			parametrToString = All.d.GetParametrToString(strXML, "OperatorID");
			if (parametrToString.errCode > 0)
			{
				All.A.CurrentStatus = "Err=" + parametrToString.errCode;
				All.A.ErrHelp = parametrToString.errStr;
				All.A.ErrCode = parametrToString.errCode;
				All.Lg.SaveTextToLog("ReportZ", strXML, StatusBarLog());
				return false;
			}
			TypErrStr typErrStr = All.l.ReturnOpenShift();
			if (typErrStr.errCode > 0)
			{
				All.A.CurrentStatus = "Err=" + typErrStr.errCode;
				All.A.ErrHelp = typErrStr.errStr;
				All.A.ErrCode = typErrStr.errCode;
				All.Lg.SaveTextToLog("ReportZ", strXML, StatusBarLog());
				return false;
			}
			if (Conversions.ToInteger(typErrStr.ReturnStr) < 1)
			{
				parametrToString.errStr = "Немає відкритої зміни.";
				parametrToString.errCode = 8;
				All.A.CurrentStatus = "Err=" + parametrToString.errCode;
				All.A.ErrHelp = parametrToString.errStr;
				All.A.ErrCode = parametrToString.errCode;
				All.Lg.SaveTextToLog("ReportZ", strXML, StatusBarLog());
				return false;
			}
			if (!All.OfflineTimeOn())
			{
				parametrToString.errStr = "Перевищено ліміт часу роботи в режимі офлайн (" + All.f.StringGetFn(All.A.FN, "LastOfflineErr") + ")";
				parametrToString.errCode = 42;
				All.A.CurrentStatus = "Err=" + parametrToString.errCode;
				All.A.ErrHelp = parametrToString.errStr;
				All.A.ErrCode = parametrToString.errCode;
				All.Lg.SaveTextToLog("ReportZ", strXML, StatusBarLog());
				return false;
			}
			if (Operators.CompareString(All.d.GetParametrToString(strXML, "shiftCashInOut").ReturnStr, "1", false) == 0)
			{
				string text = All.Bablo(All.Nal().ToString());
				string text2 = "<?xml version='1.0' encoding='UTF-8'?><InputParameters>";
				text2 = text2 + "<Parameters FN='" + returnStr + "' SumOut='" + text + "'/></InputParameters>";
				CashInOut(text2);
			}
			IniHGB iniHGB = new IniHGB(All.MyDoc() + "\\WebCheck\\Temp\\" + All.A.FN + "\\bl.ini");
			int num = 0;
			while (iniHGB.IntegerGetFn(All.A.FN, "Block") != 0)
			{
				Application.DoEvents();
				Thread.Sleep(500);
				num = checked(num + 1);
				if (num > 108)
				{
					break;
				}
			}
			if (iniHGB.IntegerGetFn(All.A.FN, "Block") == 1)
			{
				parametrToString.errStr = "База заблокированна, ведется отправка офлайн чеков.";
				parametrToString.errCode = 1015;
				All.A.CurrentStatus = "Err=" + parametrToString.errCode;
				All.A.ErrHelp = parametrToString.errStr;
				All.A.ErrCode = parametrToString.errCode;
				All.Lg.SaveTextToLog("ReportZ", strXML, StatusBarLog());
				return false;
			}
			OfflineOnZ();
			if (!All.l.BlockBase(bl: true))
			{
				All.A.CurrentStatus = "Err=" + 57;
				All.A.ErrHelp = "Превышен лимит времени ожидания доступа к базе";
				All.A.ErrCode = 57;
				All.Lg.SaveTextToLog("ReportZ", strXML, StatusBarLog());
				return false;
			}
			TypErrStr typErrStr2 = All.d.ReportXZ(typErrStr.ReturnStr, "z", typErrStr.ReturnStr);
			All.l.BlockBase(bl: false);
			if (typErrStr2.errCode == 32)
			{
				if (!All.l.BlockBase(bl: true))
				{
					All.A.CurrentStatus = "Err=" + 57;
					All.A.ErrHelp = "Превышен лимит времени ожидания доступа к базе";
					All.A.ErrCode = 57;
					All.Lg.SaveTextToLog("ReportZ", strXML, StatusBarLog());
					return false;
				}
				typErrStr2 = All.d.ReportXZ(typErrStr.ReturnStr, "z", typErrStr.ReturnStr);
				All.l.BlockBase(bl: false);
			}
			if (typErrStr2.errCode > 0)
			{
				All.A.CurrentStatus = "Err=" + typErrStr2.errCode;
				All.A.ErrHelp = typErrStr2.errStr;
				All.A.ErrCode = typErrStr2.errCode;
				All.Lg.SaveTextToLog("ReportZ", strXML, StatusBarLog());
				return false;
			}
			TypErr typErr = All.l.CloseCurrentShift();
			if (typErr.errCode > 0)
			{
				All.A.CurrentStatus = "Err=" + typErr.errCode;
				All.A.ErrHelp = typErr.errStr;
				All.A.ErrCode = typErr.errCode;
				All.Lg.SaveTextToLog("ReportZ", strXML, StatusBarLog());
				return false;
			}
			All.l.TransferBackup();
			new BackupINI().StartBackup();
			if (All.A.ShowPrintForm)
			{
				ShowPrintByCheckFn("<InputParameters><Parameters TaxNum='z' /></InputParameters>");
			}
			string returnStr2 = All.l.MaxID("ksef").ReturnStr;
			TypPrintChecks typPrintChecks = All.Rf.CheckXMLNumber(returnStr2, SearchID: true);
			new PrintExportCheck().ExportToFile(typPrintChecks.ReturnStr, typPrintChecks.ReturnStrTaxN);
			All.A.Report = typErrStr2.ReturnStr;
			All.A.CurrentStatus = "Err=0_FN=" + All.A.FN;
			All.A.ErrHelp = "";
			All.A.ErrCode = 0;
			All.Lg.SaveTextToLog("ReportZ " + now.ToString() + " - " + DateTime.Now, strXML, StatusBarLog());
			return true;
		}
		parametrToString.errCode = 4;
		parametrToString.errStr = "Нет подключения";
		All.A.CurrentStatus = "Err=" + parametrToString.errCode;
		All.A.ErrHelp = parametrToString.errStr;
		All.A.ErrCode = parametrToString.errCode;
		All.Lg.SaveTextToLog("ReportZ", strXML, StatusBarLog());
		return false;
	}

	public bool ReportX(string strXML)
	{
		DateTime now = DateTime.Now;
		TypErrStr parametrToString = All.d.GetParametrToString(strXML, "fn");
		if (parametrToString.errCode > 0)
		{
			All.A.CurrentStatus = "Err=" + parametrToString.errCode;
			All.A.ErrHelp = parametrToString.errStr;
			All.A.ErrCode = parametrToString.errCode;
			All.Lg.SaveTextToLog("ReportX", strXML, StatusBarLog());
			return false;
		}
		if (!Versioned.IsNumeric((object)parametrToString.ReturnStr) | (parametrToString.ReturnStr.Length != 10))
		{
			parametrToString.errCode = 3;
			parametrToString.errStr = "Неправильний формат Фіскального Номера: " + parametrToString.ReturnStr;
			All.A.CurrentStatus = "Err=" + parametrToString.errCode;
			All.A.ErrHelp = parametrToString.errStr;
			All.A.ErrCode = parametrToString.errCode;
			All.Lg.SaveTextToLog("ReportX", strXML, StatusBarLog());
			return false;
		}
		if (Operators.CompareString(All.f.StringGetFn(parametrToString.ReturnStr, "Path"), "", false) == 0)
		{
			parametrToString.errCode = 1001;
			parametrToString.errStr = "Фіскального номера " + parametrToString.ReturnStr + " у базі немає.";
			All.A.CurrentStatus = "Err=" + parametrToString.errCode;
			All.A.ErrCode = parametrToString.errCode;
			All.A.ErrHelp = parametrToString.errStr;
			All.Lg.SaveTextToLog("ReportX", strXML, StatusBarLog());
			return false;
		}
		if (All.f.IntegerGetFn(parametrToString.ReturnStr, "On") < 1)
		{
			parametrToString.errCode = 1;
			parametrToString.errStr = "Доступ к Фискальному Номеру " + parametrToString.ReturnStr + " запрещен.";
			All.A.CurrentStatus = "Err=" + parametrToString.errCode;
			All.A.ErrHelp = parametrToString.errStr;
			All.A.ErrCode = parametrToString.errCode;
			All.Lg.SaveTextToLog("ReportX", strXML, StatusBarLog());
			return false;
		}
		if (All.A.Status)
		{
			if (Operators.CompareString(parametrToString.ReturnStr, All.A.FN, false) != 0)
			{
				parametrToString.errCode = 2;
				parametrToString.errStr = "Ранее был подключен другой ФН.";
				All.A.CurrentStatus = "Err=" + parametrToString.errCode;
				All.A.ErrHelp = parametrToString.errStr;
				All.A.ErrCode = parametrToString.errCode;
				All.Lg.SaveTextToLog("ReportX", strXML, StatusBarLog());
				return false;
			}
			TypErrStr typErrStr = All.l.ReturnOpenShift();
			if (typErrStr.errCode > 0)
			{
				All.A.CurrentStatus = "Err=" + typErrStr.errCode;
				All.A.ErrHelp = typErrStr.errStr;
				All.A.ErrCode = typErrStr.errCode;
				All.Lg.SaveTextToLog("ReportX", strXML, StatusBarLog());
				return false;
			}
			if (Conversions.ToInteger(typErrStr.ReturnStr) < 1)
			{
				parametrToString.errStr = "Немає відкритої зміни.";
				parametrToString.errCode = 8;
				All.A.CurrentStatus = "Err=" + parametrToString.errCode;
				All.A.ErrHelp = parametrToString.errStr;
				All.A.ErrCode = parametrToString.errCode;
				All.Lg.SaveTextToLog("ReportX", strXML, StatusBarLog());
				return false;
			}
			TypErrStr typErrStr2 = All.d.ReportXZ(typErrStr.ReturnStr);
			if (typErrStr2.errCode > 0)
			{
				All.A.CurrentStatus = "Err=" + typErrStr2.errCode;
				All.A.ErrHelp = typErrStr2.errStr;
				All.A.ErrCode = typErrStr2.errCode;
				All.Lg.SaveTextToLog("ReportX", strXML, StatusBarLog());
				return false;
			}
			All.l.TransferBackup();
			if (All.A.ShowPrintFormX)
			{
				ShowPrintByCheckFn("", typErrStr2.ReturnStr);
			}
			new PrintExportCheck().ExportToFile(typErrStr2.ReturnStr, "LastX");
			All.A.Report = typErrStr2.ReturnStr;
			All.A.CurrentStatus = "Err=0_FN=" + All.A.FN;
			All.A.ErrHelp = "";
			All.A.ErrCode = 0;
			All.Lg.SaveTextToLog("ReportX " + now.ToString() + " - " + DateTime.Now, strXML, StatusBarLog());
			return true;
		}
		parametrToString.errCode = 4;
		parametrToString.errStr = "Нет подключения";
		All.A.CurrentStatus = "Err=" + parametrToString.errCode;
		All.A.ErrHelp = parametrToString.errStr;
		All.A.ErrCode = parametrToString.errCode;
		All.Lg.SaveTextToLog("ReportX", strXML, StatusBarLog());
		return false;
	}

	public bool GetCurrentStatus(string strFN)
	{
		TypErrStr parametrToString = All.d.GetParametrToString(strFN, "fn");
		if (parametrToString.errCode > 0)
		{
			All.A.CurrentStatus = "Err=" + parametrToString.errCode;
			All.A.ErrHelp = parametrToString.errStr;
			All.A.ErrCode = parametrToString.errCode;
			All.Lg.SaveTextToLog("GetCurrentStatus", strFN, StatusBarLog());
			return false;
		}
		if (!Versioned.IsNumeric((object)parametrToString.ReturnStr) | (parametrToString.ReturnStr.Length != 10))
		{
			parametrToString.errCode = 3;
			parametrToString.errStr = "Неправильний формат Фіскального Номера: " + parametrToString.ReturnStr;
			All.A.CurrentStatus = "Err=" + parametrToString.errCode;
			All.A.ErrHelp = parametrToString.errStr;
			All.A.ErrCode = parametrToString.errCode;
			All.Lg.SaveTextToLog("GetCurrentStatus", strFN, StatusBarLog());
			return false;
		}
		if (Operators.CompareString(All.f.StringGetFn(parametrToString.ReturnStr, "Path"), "", false) == 0)
		{
			parametrToString.errCode = 1001;
			parametrToString.errStr = "Фіскального номера " + parametrToString.ReturnStr + " у базі немає.";
			All.A.CurrentStatus = "Err=" + parametrToString.errCode;
			All.A.ErrCode = parametrToString.errCode;
			All.A.ErrHelp = parametrToString.errStr;
			All.Lg.SaveTextToLog("GetCurrentStatus", strFN, StatusBarLog());
			return false;
		}
		if (All.f.IntegerGetFn(parametrToString.ReturnStr, "On") < 1)
		{
			parametrToString.errCode = 1;
			parametrToString.errStr = "Доступ к Фискальному Номеру " + parametrToString.ReturnStr + " запрещен.";
			All.A.CurrentStatus = "Err=" + parametrToString.errCode;
			All.A.ErrHelp = parametrToString.errStr;
			All.A.ErrCode = parametrToString.errCode;
			All.Lg.SaveTextToLog("GetCurrentStatus", strFN, StatusBarLog());
			return false;
		}
		if (!All.A.Status)
		{
			parametrToString.errCode = 4;
			parametrToString.errStr = "Нет подключения.";
			All.A.CurrentStatus = "Err=" + parametrToString.errCode;
			All.A.ErrHelp = parametrToString.errStr;
			All.A.ErrCode = parametrToString.errCode;
			All.Lg.SaveTextToLog("GetCurrentStatus", strFN, StatusBarLog());
			return false;
		}
		TypErrStr typErrStr = All.l.ReturnOpenShift();
		if (typErrStr.errCode > 0)
		{
			All.A.CurrentStatus = "Err=" + typErrStr.errCode;
			All.A.ErrHelp = typErrStr.errStr;
			All.A.ErrCode = typErrStr.errCode;
			All.Lg.SaveTextToLog("GetCurrentStatus", strFN, StatusBarLog());
			return false;
		}
		NumbersOfflineUse numbersOfflineUse = new NumbersOfflineUse();
		string text = All.l.OfflineDate().ReturnStr.Trim();
		int num = All.RemTimeCurrentOffline();
		string text2 = ((Operators.CompareString(text, "", false) != 0) ? ("_LastOfflineStart=" + text + "_BalanceOffline=" + num) : checked((Operators.CompareString(All.A.FN, "7000000512", false) != 0) ? ("_LastOfflineStart=off_BalanceOffline=" + (10080 - All.f.IntegerGetFn(All.A.FN, "OfflineTime") - 9)) : ("_LastOfflineStart=DEMO_BalanceOffline=" + (10080 - All.f.IntegerGetFn(All.A.FN, "OfflineTime") - 9))));
		TypErrTimeShift typErrTimeShift = All.ShiftTime();
		string text3 = All.Bablo(All.Nal().ToString());
		string text4 = "_FiscalMode=0";
		if (Operators.CompareString(All.A.FiscalMode, "cabinet.tax.gov.ua:9443", false) == 0)
		{
			text4 = "_FiscalMode=0";
		}
		else if (Operators.CompareString(All.A.FiscalMode, All.URLfact, false) == 0)
		{
			text4 = "_FiscalMode=1";
		}
		string returnStr = All.l.MaxID("ksef").ReturnStr;
		returnStr = All.l.NumberTaxIDksef(returnStr).ReturnStr;
		returnStr = "_LastFiscalNumber=" + returnStr.Replace("_", "`");
		if (Conversions.ToInteger(typErrStr.ReturnStr) > 0)
		{
			TypErrLLCNshift typErrLLCNshift = All.l.ReturnLocalCheckNumberShift();
			if (typErrLLCNshift.errCode > 0)
			{
				All.A.CurrentStatus = "Err=" + typErrStr.errCode;
				All.A.ErrHelp = typErrLLCNshift.errStr;
				All.A.ErrCode = typErrLLCNshift.errCode;
				All.Lg.SaveTextToLog("GetCurrentStatus", strFN, StatusBarLog());
				return false;
			}
			string returnStr2 = All.l.INNoperatorInShift(typErrStr.ReturnStr).ReturnStr;
			string text5 = All.SF.KeyShiftEnd(returnStr2);
			All.A.CurrentStatus = "Err=0_FN=" + All.A.FN + "_ShiftNumber=" + typErrStr.ReturnStr + "_LocalCheckNumber=" + typErrLLCNshift.LastLocalCheckNumbern + "_Cashier=" + All.DelTabooS(typErrLLCNshift.Cashier) + "_OfflineCount=" + All.l.OfflineCheckCount() + "_Offline=" + All.l.OfflineTrueInt() + "_OfflinePool=" + numbersOfflineUse.CountNubmers() + text2 + "_ShiftStart=" + typErrTimeShift.returnShiftStart + "_CashBalance=" + text3 + text4 + returnStr + "_CertExpireDate=" + text5;
		}
		else
		{
			All.A.CurrentStatus = "Err=0_FN=" + All.A.FN + "_ShiftNumber=" + typErrStr.ReturnStr + "_OfflineCount=" + All.l.OfflineCheckCount() + "_Offline=" + All.l.OfflineTrueInt() + "_OfflinePool=" + numbersOfflineUse.CountNubmers() + text2 + "_ShiftStart=" + typErrTimeShift.returnShiftStart + "_CashBalance=" + text3 + text4 + returnStr;
			All.A.ErrHelp = "";
			All.A.ErrCode = 0;
		}
		All.Lg.SaveTextToLog("GetCurrentStatus", strFN, StatusBarLog());
		return true;
	}

	public bool FiscalReceipt(string strXML)
	{
		if (All.A.CheckTimeLog)
		{
			All.Lg.SaveTextToLog("TimeControl", "Вход", "Внимание! Включена функция контроля времени.");
		}
		DateTime now = DateTime.Now;
		All.SecondRunControl();
		strXML = All.d.TegXml(strXML);
		TypErrStr parametrToString = All.d.GetParametrToString(strXML, "fn", "check", RegUpLow: true);
		if (parametrToString.errCode > 0)
		{
			All.A.CurrentStatus = "Err=" + parametrToString.errCode;
			All.A.ErrHelp = parametrToString.errStr;
			All.A.ErrCode = parametrToString.errCode;
			All.Lg.SaveTextToLog("FiscalReceipt", strXML, StatusBarLog());
			return false;
		}
		if (!Versioned.IsNumeric((object)parametrToString.ReturnStr) | (parametrToString.ReturnStr.Length != 10))
		{
			parametrToString.errCode = 3;
			parametrToString.errStr = "Неправильний формат Фіскального Номера: " + parametrToString.ReturnStr;
			All.A.CurrentStatus = "Err=" + parametrToString.errCode;
			All.A.ErrHelp = parametrToString.errStr;
			All.A.ErrCode = parametrToString.errCode;
			All.Lg.SaveTextToLog("FiscalReceipt", strXML, StatusBarLog());
			return false;
		}
		TypErrStr typErrStr = All.ReplacingAMP(strXML);
		if (typErrStr.errCode > 0)
		{
			All.A.CurrentStatus = "Err=" + typErrStr.errCode;
			All.A.ErrHelp = typErrStr.errStr;
			All.A.ErrCode = typErrStr.errCode;
			All.Lg.SaveTextToLog("FiscalReceipt", strXML, StatusBarLog());
			return false;
		}
		strXML = typErrStr.ReturnStr;
		if (Operators.CompareString(All.f.StringGetFn(parametrToString.ReturnStr, "Path"), "", false) == 0)
		{
			parametrToString.errCode = 1001;
			parametrToString.errStr = "Фіскального номера " + parametrToString.ReturnStr + " у базі немає.";
			All.A.CurrentStatus = "Err=" + parametrToString.errCode;
			All.A.ErrCode = parametrToString.errCode;
			All.A.ErrHelp = parametrToString.errStr;
			All.Lg.SaveTextToLog("FiscalReceipt", strXML, StatusBarLog());
			return false;
		}
		if (All.f.IntegerGetFn(parametrToString.ReturnStr, "On") < 1)
		{
			parametrToString.errCode = 1;
			parametrToString.errStr = "Доступ к Фискальному Номеру " + parametrToString.ReturnStr + " запрещен.";
			All.A.CurrentStatus = "Err=" + parametrToString.errCode;
			All.A.ErrHelp = parametrToString.errStr;
			All.A.ErrCode = parametrToString.errCode;
			All.Lg.SaveTextToLog("FiscalReceipt", strXML, StatusBarLog());
			return false;
		}
		if (All.A.Status && Operators.CompareString(parametrToString.ReturnStr, All.A.FN, false) != 0)
		{
			parametrToString.errCode = 2;
			parametrToString.errStr = "Подключен другой ФН.";
			All.A.CurrentStatus = "Err=" + parametrToString.errCode;
			All.A.ErrHelp = parametrToString.errStr;
			All.A.ErrCode = parametrToString.errCode;
			All.Lg.SaveTextToLog("FiscalReceipt", strXML, StatusBarLog());
			return false;
		}
		if (!All.A.Status)
		{
			parametrToString.errCode = 4;
			parametrToString.errStr = "Нет подключения.";
			All.A.CurrentStatus = "Err=" + parametrToString.errCode;
			All.A.ErrHelp = parametrToString.errStr;
			All.A.ErrCode = parametrToString.errCode;
			All.Lg.SaveTextToLog("FiscalReceipt", strXML, StatusBarLog());
			return false;
		}
		TypErrStr typErrStr2 = All.l.ReturnOpenShift();
		if (typErrStr2.errCode > 0)
		{
			All.A.CurrentStatus = "Err=" + typErrStr2.errCode;
			All.A.ErrHelp = typErrStr2.errStr;
			All.A.ErrCode = typErrStr2.errCode;
			All.Lg.SaveTextToLog("FiscalReceipt", strXML, StatusBarLog());
			return false;
		}
		if (Conversions.ToInteger(typErrStr2.ReturnStr) < 1)
		{
			parametrToString.errStr = "Немає відкритої зміни.";
			parametrToString.errCode = 8;
			All.A.CurrentStatus = "Err=" + parametrToString.errCode;
			All.A.ErrHelp = parametrToString.errStr;
			All.A.ErrCode = parametrToString.errCode;
			All.Lg.SaveTextToLog("FiscalReceipt", strXML, StatusBarLog());
			return false;
		}
		if (!All.OfflineTimeOn())
		{
			parametrToString.errStr = "Перевищено ліміт часу роботи в режимі офлайн (" + All.f.StringGetFn(All.A.FN, "LastOfflineErr") + ")";
			parametrToString.errCode = 42;
			All.A.CurrentStatus = "Err=" + parametrToString.errCode;
			All.A.ErrHelp = parametrToString.errStr;
			All.A.ErrCode = parametrToString.errCode;
			All.Lg.SaveTextToLog("FiscalReceipt", strXML, StatusBarLog());
			return false;
		}
		TypErrTimeShift typErrTimeShift = All.ShiftTime();
		if (typErrTimeShift.errCode > 0)
		{
			parametrToString.errStr = typErrTimeShift.errStr;
			parametrToString.errCode = typErrTimeShift.errCode;
			All.A.CurrentStatus = "Err=" + parametrToString.errCode;
			All.A.ErrHelp = parametrToString.errStr;
			All.A.ErrCode = parametrToString.errCode;
			All.Lg.SaveTextToLog("FiscalReceipt", strXML, StatusBarLog());
			return false;
		}
		IniHGB iniHGB = new IniHGB(All.MyDoc() + "\\WebCheck\\Temp\\" + All.A.FN + "\\bl.ini");
		int num = 0;
		while (iniHGB.IntegerGetFn(All.A.FN, "Block") != 0)
		{
			Application.DoEvents();
			Thread.Sleep(500);
			num = checked(num + 1);
			if (num > 108)
			{
				break;
			}
		}
		if (iniHGB.IntegerGetFn(All.A.FN, "Block") == 1)
		{
			parametrToString.errStr = "База заблокированна, ведется отправка офлайн чеков.";
			parametrToString.errCode = 1015;
			All.A.CurrentStatus = "Err=" + parametrToString.errCode;
			All.A.ErrHelp = parametrToString.errStr;
			All.A.ErrCode = parametrToString.errCode;
			All.Lg.SaveTextToLog("FiscalReceipt", strXML, StatusBarLog());
			return false;
		}
		OfflineOnC();
		if (!All.l.BlockBase(bl: true))
		{
			All.A.CurrentStatus = "Err=" + 57;
			All.A.ErrHelp = "Превышен лимит времени ожидания доступа к базе";
			All.A.ErrCode = 57;
			All.Lg.SaveTextToLog("FiscalReceipt", strXML, StatusBarLog());
			return false;
		}
		parametrToString = All.d.CheckProcessing(strXML, typErrStr2.ReturnStr);
		All.l.BlockBase(bl: false);
		if (parametrToString.errCode == 32)
		{
			if (!All.l.BlockBase(bl: true))
			{
				All.A.CurrentStatus = "Err=" + 57;
				All.A.ErrHelp = "Превышен лимит времени ожидания доступа к базе";
				All.A.ErrCode = 57;
				All.Lg.SaveTextToLog("FiscalReceipt", strXML, StatusBarLog());
				return false;
			}
			parametrToString = All.d.CheckProcessing(strXML, typErrStr2.ReturnStr);
			All.l.BlockBase(bl: false);
		}
		if (parametrToString.errCode > 0)
		{
			All.A.CurrentStatus = "Err=" + parametrToString.errCode;
			All.A.ErrHelp = parametrToString.errStr;
			All.A.ErrCode = parametrToString.errCode;
			All.Lg.SaveTextToLog("FiscalReceipt", strXML, StatusBarLog());
			return false;
		}
		All.l.TransferBackup();
		if (All.A.ShowPrintForm)
		{
			ShowPrintByCheckFn();
		}
		string returnStr = All.l.MaxID("ksef").ReturnStr;
		TypPrintChecks typPrintChecks = All.Rf.CheckXMLNumber(returnStr, SearchID: true);
		new PrintExportCheck().ExportToFile(typPrintChecks.ReturnStr, typPrintChecks.ReturnStrTaxN);
		All.A.CurrentStatus = "Err=0_FN=" + All.A.FN + parametrToString.ReturnStr;
		All.A.ErrHelp = "";
		All.A.ErrCode = 0;
		All.Lg.SaveTextToLog("FiscalReceipt " + now.ToString() + " - " + DateTime.Now, strXML, StatusBarLog());
		if (All.A.CheckTimeLog)
		{
			All.Lg.SaveTextToLog("TimeControl", "Выход");
		}
		return true;
	}

	public bool CashInOut(string strFN)
	{
		All.SecondRunControl();
		TypErrStr parametrToString = All.d.GetParametrToString(strFN, "fn");
		if (parametrToString.errCode > 0)
		{
			All.A.CurrentStatus = "Err=" + parametrToString.errCode;
			All.A.ErrHelp = parametrToString.errStr;
			All.A.ErrCode = parametrToString.errCode;
			All.Lg.SaveTextToLog("CashInOut", strFN, StatusBarLog());
			return false;
		}
		if (!Versioned.IsNumeric((object)parametrToString.ReturnStr) | (parametrToString.ReturnStr.Length != 10))
		{
			parametrToString.errCode = 3;
			parametrToString.errStr = "Неправильний формат Фіскального Номера: " + parametrToString.ReturnStr;
			All.A.CurrentStatus = "Err=" + parametrToString.errCode;
			All.A.ErrHelp = parametrToString.errStr;
			All.A.ErrCode = parametrToString.errCode;
			All.Lg.SaveTextToLog("CashInOut", strFN, StatusBarLog());
			return false;
		}
		if (Operators.CompareString(All.f.StringGetFn(parametrToString.ReturnStr, "Path"), "", false) == 0)
		{
			parametrToString.errCode = 1001;
			parametrToString.errStr = "Фіскального номера " + parametrToString.ReturnStr + " у базі немає.";
			All.A.CurrentStatus = "Err=" + parametrToString.errCode;
			All.A.ErrCode = parametrToString.errCode;
			All.A.ErrHelp = parametrToString.errStr;
			All.Lg.SaveTextToLog("CashInOut", strFN, StatusBarLog());
			return false;
		}
		if (All.f.IntegerGetFn(parametrToString.ReturnStr, "On") < 1)
		{
			parametrToString.errCode = 1;
			parametrToString.errStr = "Доступ Фискальному Номеру " + parametrToString.ReturnStr + " запрещен.";
			All.A.CurrentStatus = "Err=" + parametrToString.errCode;
			All.A.ErrHelp = parametrToString.errStr;
			All.A.ErrCode = parametrToString.errCode;
			All.Lg.SaveTextToLog("CashInOut", strFN, StatusBarLog());
			return false;
		}
		if (All.A.Status && Operators.CompareString(parametrToString.ReturnStr, All.A.FN, false) != 0)
		{
			parametrToString.errCode = 2;
			parametrToString.errStr = "Подключен другой ФН.";
			All.A.CurrentStatus = "Err=" + parametrToString.errCode;
			All.A.ErrHelp = parametrToString.errStr;
			All.A.ErrCode = parametrToString.errCode;
			All.Lg.SaveTextToLog("CashInOut", strFN, StatusBarLog());
			return false;
		}
		if (!All.A.Status)
		{
			parametrToString.errCode = 4;
			parametrToString.errStr = "Нет подключения.";
			All.A.CurrentStatus = "Err=" + parametrToString.errCode;
			All.A.ErrHelp = parametrToString.errStr;
			All.A.ErrCode = parametrToString.errCode;
			All.Lg.SaveTextToLog("CashInOut", strFN, StatusBarLog());
			return false;
		}
		TypErrStr typErrStr = All.l.ReturnOpenShift();
		if (typErrStr.errCode > 0)
		{
			All.A.CurrentStatus = "Err=" + typErrStr.errCode;
			All.A.ErrHelp = typErrStr.errStr;
			All.A.ErrCode = typErrStr.errCode;
			All.Lg.SaveTextToLog("CashInOut", strFN, StatusBarLog());
			return false;
		}
		if (Conversions.ToInteger(typErrStr.ReturnStr) < 1)
		{
			parametrToString.errStr = "Немає відкритої зміни.";
			parametrToString.errCode = 8;
			All.A.CurrentStatus = "Err=" + parametrToString.errCode;
			All.A.ErrHelp = parametrToString.errStr;
			All.A.ErrCode = parametrToString.errCode;
			All.Lg.SaveTextToLog("CashInOut", strFN, StatusBarLog());
			return false;
		}
		if (!All.OfflineTimeOn())
		{
			parametrToString.errStr = "Перевищено ліміт часу роботи в режимі офлайн (" + All.f.StringGetFn(All.A.FN, "LastOfflineErr") + ")";
			parametrToString.errCode = 42;
			All.A.CurrentStatus = "Err=" + parametrToString.errCode;
			All.A.ErrHelp = parametrToString.errStr;
			All.A.ErrCode = parametrToString.errCode;
			All.Lg.SaveTextToLog("CashInOut", strFN, StatusBarLog());
			return false;
		}
		TypErrTimeShift typErrTimeShift = All.ShiftTime();
		if (typErrTimeShift.errCode > 0)
		{
			parametrToString.errStr = typErrTimeShift.errStr;
			parametrToString.errCode = typErrTimeShift.errCode;
			All.A.CurrentStatus = "Err=" + parametrToString.errCode;
			All.A.ErrHelp = parametrToString.errStr;
			All.A.ErrCode = parametrToString.errCode;
			All.Lg.SaveTextToLog("CashInOut", strFN, StatusBarLog());
			return false;
		}
		IniHGB iniHGB = new IniHGB(All.MyDoc() + "\\WebCheck\\Temp\\" + All.A.FN + "\\bl.ini");
		int num = 0;
		while (iniHGB.IntegerGetFn(All.A.FN, "Block") != 0)
		{
			Application.DoEvents();
			Thread.Sleep(500);
			num = checked(num + 1);
			if (num > 108)
			{
				break;
			}
		}
		if (iniHGB.IntegerGetFn(All.A.FN, "Block") == 1)
		{
			parametrToString.errStr = "База заблокированна, ведется отправка офлайн чеков.";
			parametrToString.errCode = 1015;
			All.A.CurrentStatus = "Err=" + parametrToString.errCode;
			All.A.ErrHelp = parametrToString.errStr;
			All.A.ErrCode = parametrToString.errCode;
			All.Lg.SaveTextToLog("CashInOut", strFN, StatusBarLog());
			return false;
		}
		if (!All.l.BlockBase(bl: true))
		{
			All.A.CurrentStatus = "Err=" + 57;
			All.A.ErrHelp = "Превышен лимит времени ожидания доступа к базе";
			All.A.ErrCode = 57;
			All.Lg.SaveTextToLog("CashInOut", strFN, StatusBarLog());
			return false;
		}
		parametrToString = All.d.DealCheck(strFN, typErrStr.ReturnStr);
		All.l.BlockBase(bl: false);
		if (parametrToString.errCode == 32)
		{
			if (!All.l.BlockBase(bl: true))
			{
				All.A.CurrentStatus = "Err=" + 57;
				All.A.ErrHelp = "Превышен лимит времени ожидания доступа к базе";
				All.A.ErrCode = 57;
				All.Lg.SaveTextToLog("CashInOut", strFN, StatusBarLog());
				return false;
			}
			parametrToString = All.d.DealCheck(strFN, typErrStr.ReturnStr);
			All.l.BlockBase(bl: false);
		}
		if (parametrToString.errCode > 0)
		{
			All.A.CurrentStatus = "Err=" + parametrToString.errCode;
			All.A.ErrHelp = parametrToString.errStr;
			All.A.ErrCode = parametrToString.errCode;
			All.Lg.SaveTextToLog("CashInOut", strFN, StatusBarLog());
			return false;
		}
		All.l.TransferBackup();
		if (All.A.ShowPrintForm)
		{
			ShowPrintByCheckFn();
		}
		string returnStr = All.l.MaxID("ksef").ReturnStr;
		TypPrintChecks typPrintChecks = All.Rf.CheckXMLNumber(returnStr, SearchID: true);
		new PrintExportCheck().ExportToFile(typPrintChecks.ReturnStr, typPrintChecks.ReturnStrTaxN);
		All.A.CurrentStatus = "Err=0_FN=" + All.A.FN + parametrToString.ReturnStr;
		All.A.ErrHelp = "";
		All.A.ErrCode = 0;
		All.Lg.SaveTextToLog("CashInOut", strFN, StatusBarLog());
		return true;
	}

	public bool EPZtoCash(string strFN)
	{
		strFN = strFN.ToLower();
		TypErrStr parametrToString = All.d.GetParametrToString(strFN, "fn");
		if (parametrToString.errCode > 0)
		{
			All.A.ErrHelp = "Ошибка параметра fn";
			All.A.ErrCode = 94;
			All.A.CurrentStatus = "Err=" + All.A.ErrCode;
			All.Lg.SaveTextToLog("EPZtoCash", strFN, StatusBarLog());
			return false;
		}
		string returnStr = parametrToString.ReturnStr;
		parametrToString = All.d.GetParametrToString(strFN, "sum");
		if (parametrToString.errCode > 0)
		{
			All.A.ErrHelp = "Ошибка параметра sum";
			All.A.ErrCode = 94;
			All.A.CurrentStatus = "Err=" + All.A.ErrCode;
			All.Lg.SaveTextToLog("EPZtoCash", strFN, StatusBarLog());
			return false;
		}
		string returnStr2 = parametrToString.ReturnStr;
		parametrToString = All.d.GetParametrToString(strFN, "pa");
		if (parametrToString.errCode > 0)
		{
			All.A.ErrHelp = "Ошибка параметра pa";
			All.A.ErrCode = 94;
			All.A.CurrentStatus = "Err=" + All.A.ErrCode;
			All.Lg.SaveTextToLog("EPZtoCash", strFN, StatusBarLog());
			return false;
		}
		string returnStr3 = parametrToString.ReturnStr;
		parametrToString = All.d.GetParametrToString(strFN, "pb");
		if (parametrToString.errCode > 0)
		{
			All.A.ErrHelp = "Ошибка параметра pb";
			All.A.ErrCode = 94;
			All.A.CurrentStatus = "Err=" + All.A.ErrCode;
			All.Lg.SaveTextToLog("EPZtoCash", strFN, StatusBarLog());
			return false;
		}
		string returnStr4 = parametrToString.ReturnStr;
		parametrToString = All.d.GetParametrToString(strFN, "pc");
		if (parametrToString.errCode > 0)
		{
			All.A.ErrHelp = "Ошибка параметра pc";
			All.A.ErrCode = 94;
			All.A.CurrentStatus = "Err=" + All.A.ErrCode;
			All.Lg.SaveTextToLog("EPZtoCash", strFN, StatusBarLog());
			return false;
		}
		string returnStr5 = parametrToString.ReturnStr;
		parametrToString = All.d.GetParametrToString(strFN, "pd");
		if (parametrToString.errCode > 0)
		{
			All.A.ErrHelp = "Ошибка параметра pd";
			All.A.ErrCode = 94;
			All.A.CurrentStatus = "Err=" + All.A.ErrCode;
			All.Lg.SaveTextToLog("EPZtoCash", strFN, StatusBarLog());
			return false;
		}
		string returnStr6 = parametrToString.ReturnStr;
		parametrToString = All.d.GetParametrToString(strFN, "pe");
		if (parametrToString.errCode > 0)
		{
			All.A.ErrHelp = "Ошибка параметра pe";
			All.A.ErrCode = 94;
			All.A.CurrentStatus = "Err=" + All.A.ErrCode;
			All.Lg.SaveTextToLog("EPZtoCash", strFN, StatusBarLog());
			return false;
		}
		string returnStr7 = parametrToString.ReturnStr;
		parametrToString = All.d.GetParametrToString(strFN, "pf");
		string text = ((parametrToString.errCode <= 0) ? parametrToString.ReturnStr : "");
		parametrToString = All.d.GetParametrToString(strFN, "psnm");
		if (parametrToString.errCode > 0)
		{
			All.A.ErrHelp = "Ошибка параметра psnm";
			All.A.ErrCode = 94;
			All.A.CurrentStatus = "Err=" + All.A.ErrCode;
			All.Lg.SaveTextToLog("EPZtoCash", strFN, StatusBarLog());
			return false;
		}
		string returnStr8 = parametrToString.ReturnStr;
		parametrToString = All.d.GetParametrToString(strFN, "rrn");
		if (parametrToString.errCode > 0)
		{
			All.A.ErrHelp = "Ошибка параметра rrn";
			All.A.ErrCode = 94;
			All.A.CurrentStatus = "Err=" + All.A.ErrCode;
			All.Lg.SaveTextToLog("EPZtoCash", strFN, StatusBarLog());
			return false;
		}
		string returnStr9 = parametrToString.ReturnStr;
		parametrToString = All.d.GetParametrToString(strFN, "paymentid");
		if (parametrToString.errCode > 0)
		{
			All.A.ErrHelp = "Ошибка параметра paymentid";
			All.A.ErrCode = 94;
			All.A.CurrentStatus = "Err=" + All.A.ErrCode;
			All.Lg.SaveTextToLog("EPZtoCash", strFN, StatusBarLog());
			return false;
		}
		string returnStr10 = parametrToString.ReturnStr;
		if (!Versioned.IsNumeric((object)returnStr10))
		{
			All.A.ErrHelp = "Параметр paymentid не является числом";
			All.A.ErrCode = 94;
			All.A.CurrentStatus = "Err=" + All.A.ErrCode;
			All.Lg.SaveTextToLog("EPZtoCash", strFN, StatusBarLog());
			return false;
		}
		if (Conversions.ToInteger(returnStr10) < 2)
		{
			All.A.ErrHelp = "Тип paymentid не может быть меньше 2";
			All.A.ErrCode = 94;
			All.A.CurrentStatus = "Err=" + All.A.ErrCode;
			All.Lg.SaveTextToLog("EPZtoCash", strFN, StatusBarLog());
			return false;
		}
		if (All.StrToDouble(returnStr2) > All.Nal())
		{
			All.A.ErrHelp = "Помилка! У касі немає необхідної суми.";
			All.A.ErrCode = 47;
			All.A.CurrentStatus = "Err=" + All.A.ErrCode;
			All.Lg.SaveTextToLog("EPZtoCash", strFN, StatusBarLog());
			return false;
		}
		string text2 = "<?xml version='1.0' encoding='windows-1251'?>";
		text2 = text2 + "<check fn='" + returnStr + "' operationtype='-8'> <checkhead><ver>1</ver></checkhead>";
		string text3 = "<L paymentid='" + returnStr10 + "' pf='" + text + "' sum='" + returnStr2 + "' pa='";
		text3 = text3 + returnStr3 + "' pb='" + returnStr4 + "' pc='" + returnStr5 + "' pd='" + returnStr6 + "' pe='" + returnStr7 + "' psnm='" + returnStr8 + "' rrn='" + returnStr9 + "'/>";
		text2 += text3;
		text2 = text2 + "<payments><payment id='" + returnStr10 + "' sum='" + returnStr2 + "'/></payments>";
		text2 = text2 + "<goods><good code='0' name='ОПЕРАЦІЯ З ВИДАЧІ ГОТІВКОВИХ КОШТІВ ДЕРЖАТЕЛЮ ЕЛЕКТРОННОГО ПЛАТІЖНОГО ЗАСОБУ' sum='" + returnStr2 + "' quantity='1' price='" + returnStr2 + "' taxrate='2'/></goods></check>";
		if (!FiscalReceipt(text2))
		{
			return false;
		}
		return true;
	}

	public string StatusBarXML()
	{
		All.l.OpenShiftReturn = true;
		if (Operators.CompareString(All.A.Report, "", false) != 0)
		{
			string report = All.A.Report;
			All.A.Report = "";
			All.Lg.SaveTextToLog("StatusBarXML", "Status request", "OK! Z or X report");
			return report;
		}
		TypErrStr parametrToXML = default(TypErrStr);
		parametrToXML.errCode = 0;
		parametrToXML.errStr = "";
		parametrToXML.ReturnStr = All.A.CurrentStatus;
		if (All.A.ErrCode > 0)
		{
			ref string returnStr = ref parametrToXML.ReturnStr;
			returnStr = returnStr + "_ErrHelp=" + All.A.ErrHelp + "_version=" + All.VersionDll();
		}
		parametrToXML = All.d.GetParametrToXML(parametrToXML.ReturnStr);
		if (Operators.CompareString(parametrToXML.ReturnStr.Trim(), "", false) == 0)
		{
			All.Lg.SaveTextToLog("StatusBarXML", "Status request", "Error! Empty string");
		}
		else
		{
			All.Lg.SaveTextToLog("StatusBarXML", "Status request", "OK");
		}
		return parametrToXML.ReturnStr;
	}

	private string StatusBarLog()
	{
		All.l.OpenShiftReturn = true;
		if (Operators.CompareString(All.A.Report, "", false) != 0)
		{
			return All.A.Report;
		}
		TypErrStr parametrToXML = default(TypErrStr);
		parametrToXML.errCode = 0;
		parametrToXML.errStr = "";
		parametrToXML.ReturnStr = All.A.CurrentStatus;
		if (All.A.ErrCode > 0)
		{
			ref string returnStr = ref parametrToXML.ReturnStr;
			returnStr = returnStr + "_ErrHelp=" + All.A.ErrHelp + "_version=" + All.VersionDll();
		}
		parametrToXML = All.d.GetParametrToXML(parametrToXML.ReturnStr);
		return parametrToXML.ReturnStr;
	}

	public bool OfflineToOnline(string strFN)
	{
		TypErrStr parametrToString = All.d.GetParametrToString(strFN, "fn");
		if (parametrToString.errCode > 0)
		{
			All.A.CurrentStatus = "Err=" + parametrToString.errCode;
			All.A.ErrHelp = parametrToString.errStr;
			All.A.ErrCode = parametrToString.errCode;
			All.Lg.SaveTextToLog("OfflineToOnline", strFN, StatusBarLog());
			return false;
		}
		if (!Versioned.IsNumeric((object)parametrToString.ReturnStr) | (parametrToString.ReturnStr.Length != 10))
		{
			parametrToString.errCode = 3;
			parametrToString.errStr = "Неправильний формат Фіскального Номера: " + parametrToString.ReturnStr;
			All.A.CurrentStatus = "Err=" + parametrToString.errCode;
			All.A.ErrHelp = parametrToString.errStr;
			All.A.ErrCode = parametrToString.errCode;
			All.Lg.SaveTextToLog("OfflineToOnline", strFN, StatusBarLog());
			return false;
		}
		if (Operators.CompareString(All.f.StringGetFn(parametrToString.ReturnStr, "Path"), "", false) == 0)
		{
			parametrToString.errCode = 1001;
			parametrToString.errStr = "Фіскального номера " + parametrToString.ReturnStr + " у базі немає.";
			All.A.CurrentStatus = "Err=" + parametrToString.errCode;
			All.A.ErrCode = parametrToString.errCode;
			All.A.ErrHelp = parametrToString.errStr;
			All.Lg.SaveTextToLog("OfflineToOnline", strFN, StatusBarLog());
			return false;
		}
		if (All.f.IntegerGetFn(parametrToString.ReturnStr, "On") < 1)
		{
			parametrToString.errCode = 1;
			parametrToString.errStr = "Доступ Фискальному Номеру " + parametrToString.ReturnStr + " запрещен.";
			All.A.CurrentStatus = "Err=" + parametrToString.errCode;
			All.A.ErrHelp = parametrToString.errStr;
			All.A.ErrCode = parametrToString.errCode;
			All.Lg.SaveTextToLog("OfflineToOnline", strFN, StatusBarLog());
			return false;
		}
		if (All.A.Status)
		{
			if (Operators.CompareString(parametrToString.ReturnStr, All.A.FN, false) != 0)
			{
				parametrToString.errCode = 2;
				parametrToString.errStr = "Ранее был подключен другой ФН.";
				All.A.CurrentStatus = "Err=" + parametrToString.errCode;
				All.A.ErrHelp = parametrToString.errStr;
				All.A.ErrCode = parametrToString.errCode;
				All.Lg.SaveTextToLog("OfflineToOnline", strFN, StatusBarLog());
				return false;
			}
			if (!All.A.FullVersion)
			{
				parametrToString.errStr = "В бесплатной версии нет режима оффлайн.";
				parametrToString.errCode = 37;
				All.A.ErrHelp = parametrToString.errStr;
				All.A.ErrCode = parametrToString.errCode;
				All.A.CurrentStatus = "Err=" + parametrToString.errCode;
				All.Lg.SaveTextToLog("OfflineToOnline", strFN, StatusBarLog());
				return false;
			}
			if (All.A.AutomatOfflineOn)
			{
				parametrToString.errCode = 55;
				parametrToString.errStr = "Работает автоматический режим офлайн";
				All.A.ErrHelp = parametrToString.errStr;
				All.A.ErrCode = parametrToString.errCode;
				All.A.CurrentStatus = "Err=" + parametrToString.errCode;
				All.Lg.SaveTextToLog("OfflineToOnline", strFN, StatusBarLog());
				return false;
			}
			SendingOfflineChecks sendingOfflineChecks = new SendingOfflineChecks();
			All.l.OpenShiftReturn = false;
			if (!All.l.OfflineTrue())
			{
				parametrToString.errStr = "Онлайн режим уже включен.";
				parametrToString.errCode = 37;
				All.A.ErrHelp = parametrToString.errStr;
				All.A.ErrCode = parametrToString.errCode;
				All.A.CurrentStatus = "Err=" + parametrToString.errCode;
				All.Lg.SaveTextToLog("OfflineToOnline", strFN, StatusBarLog());
				All.l.OpenShiftReturn = true;
				return false;
			}
			if (All.l.OfflineCheckCount() > 0)
			{
				if (!All.l.BlockBase(bl: true))
				{
					All.A.CurrentStatus = "Err=" + 57;
					All.A.ErrHelp = "Превышен лимит времени ожидания доступа к базе";
					All.A.ErrCode = 57;
					All.Lg.SaveTextToLog("OfflineToOnline", strFN, StatusBarLog());
					return false;
				}
				TypErrStr typErrStr = sendingOfflineChecks.SendDoc();
				All.l.BlockBase(bl: false);
				if (typErrStr.errCode > 0)
				{
					All.A.ErrHelp = typErrStr.errStr;
					All.A.ErrCode = typErrStr.errCode;
					All.A.CurrentStatus = "Err=" + typErrStr.errCode;
					All.Lg.SaveTextToLog("OfflineToOnline", strFN, StatusBarLog());
					All.l.OpenShiftReturn = true;
					return false;
				}
				parametrToString.ReturnStr = typErrStr.ReturnStr;
			}
			else
			{
				TypErrStr typErrStr2 = sendingOfflineChecks.CloseOfflineDoc();
				if (typErrStr2.errCode > 0)
				{
					All.A.CurrentStatus = "Err=" + typErrStr2.errCode;
					All.A.ErrHelp = typErrStr2.errStr;
					All.A.ErrCode = typErrStr2.errCode;
					All.Lg.SaveTextToLog("OfflineToOnline", strFN, StatusBarLog());
					All.l.OpenShiftReturn = true;
					return false;
				}
				parametrToString.ReturnStr = typErrStr2.ReturnStr;
				string returnStr = All.l.MaxID("ksef").ReturnStr;
				TypPrintChecks typPrintChecks = All.Rf.CheckXMLNumber(returnStr, SearchID: true);
				new PrintExportCheck().ExportToFile(typPrintChecks.ReturnStr, typPrintChecks.ReturnStrTaxN);
				new Offlin().NewBackupNumbers();
			}
			All.l.OpenShiftReturn = true;
			All.A.CurrentStatus = "Err=0_FN=" + All.A.FN + parametrToString.ReturnStr;
			All.A.ErrHelp = "";
			All.A.ErrCode = 0;
			All.Lg.SaveTextToLog("OfflineToOnline", strFN, StatusBarLog());
			return true;
		}
		parametrToString.errCode = 4;
		parametrToString.errStr = "Нет подключения";
		All.A.ErrHelp = parametrToString.errStr;
		All.A.ErrCode = parametrToString.errCode;
		All.A.CurrentStatus = "Err=" + parametrToString.errCode;
		All.Lg.SaveTextToLog("OfflineToOnline", strFN, StatusBarLog());
		return false;
	}

	public bool GetOfflineNumbers(string strXML)
	{
		TypErrStr parametrToString = All.d.GetParametrToString(strXML, "fn");
		if (parametrToString.errCode > 0)
		{
			All.A.CurrentStatus = "Err=" + parametrToString.errCode;
			All.A.ErrHelp = parametrToString.errStr;
			All.A.ErrCode = parametrToString.errCode;
			All.Lg.SaveTextToLog("GetOfflineNumbers", strXML, StatusBarLog());
			return false;
		}
		if (!Versioned.IsNumeric((object)parametrToString.ReturnStr) | (parametrToString.ReturnStr.Length != 10))
		{
			parametrToString.errCode = 3;
			parametrToString.errStr = "Неправильний формат Фіскального Номера: " + parametrToString.ReturnStr;
			All.A.CurrentStatus = "Err=" + parametrToString.errCode;
			All.A.ErrHelp = parametrToString.errStr;
			All.A.ErrCode = parametrToString.errCode;
			All.Lg.SaveTextToLog("GetOfflineNumbers", strXML, StatusBarLog());
			return false;
		}
		if (Operators.CompareString(All.f.StringGetFn(parametrToString.ReturnStr, "Path"), "", false) == 0)
		{
			parametrToString.errCode = 1001;
			parametrToString.errStr = "Фіскального номера " + parametrToString.ReturnStr + " у базі немає.";
			All.A.CurrentStatus = "Err=" + parametrToString.errCode;
			All.A.ErrCode = parametrToString.errCode;
			All.A.ErrHelp = parametrToString.errStr;
			All.Lg.SaveTextToLog("GetOfflineNumbers", strXML, StatusBarLog());
			return false;
		}
		if (All.f.IntegerGetFn(parametrToString.ReturnStr, "On") < 1)
		{
			parametrToString.errCode = 1;
			parametrToString.errStr = "Доступ Фискальному Номеру " + parametrToString.ReturnStr + " запрещен.";
			All.A.CurrentStatus = "Err=" + parametrToString.errCode;
			All.A.ErrHelp = parametrToString.errStr;
			All.A.ErrCode = parametrToString.errCode;
			All.Lg.SaveTextToLog("GetOfflineNumbers", strXML, StatusBarLog());
			return false;
		}
		if (All.A.Status)
		{
			if (Operators.CompareString(parametrToString.ReturnStr, All.A.FN, false) != 0)
			{
				parametrToString.errCode = 2;
				parametrToString.errStr = "Ранее был подключен другой ФН.";
				All.A.CurrentStatus = "Err=" + parametrToString.errCode;
				All.A.ErrHelp = parametrToString.errStr;
				All.A.ErrCode = parametrToString.errCode;
				All.Lg.SaveTextToLog("GetOfflineNumbers", strXML, StatusBarLog());
				return false;
			}
			if (!All.A.FullVersion)
			{
				parametrToString.errStr = "В бесплатной версии нет режима оффлайн.";
				parametrToString.errCode = 37;
				All.A.CurrentStatus = "Err=" + parametrToString.errCode;
				All.A.ErrHelp = parametrToString.errStr;
				All.A.ErrCode = parametrToString.errCode;
				All.Lg.SaveTextToLog("GetOfflineNumbers", strXML, StatusBarLog());
				return false;
			}
			if (All.l.OfflineTrue())
			{
				parametrToString.errStr = "Включен оффлайн режим.";
				parametrToString.errCode = 36;
				All.A.CurrentStatus = "Err=" + parametrToString.errCode;
				All.A.ErrHelp = parametrToString.errStr;
				All.A.ErrCode = parametrToString.errCode;
				All.Lg.SaveTextToLog("GetOfflineNumbers", strXML, StatusBarLog());
				return false;
			}
			if (!All.OfflineTimeOn())
			{
				parametrToString.errStr = "Перевищено ліміт часу роботи в режимі офлайн (" + All.f.StringGetFn(All.A.FN, "LastOfflineErr") + ")";
				parametrToString.errCode = 42;
				All.A.CurrentStatus = "Err=" + parametrToString.errCode;
				All.A.ErrHelp = parametrToString.errStr;
				All.A.ErrCode = parametrToString.errCode;
				All.Lg.SaveTextToLog("GetOfflineNumbers", strXML, StatusBarLog());
				return false;
			}
			if (All.A.AutomatOfflineOn)
			{
				parametrToString.errCode = 55;
				parametrToString.errStr = "Работает автоматический режим офлайн";
				All.A.ErrHelp = parametrToString.errStr;
				All.A.ErrCode = parametrToString.errCode;
				All.A.CurrentStatus = "Err=" + parametrToString.errCode;
				All.Lg.SaveTextToLog("OfflineToOnline", strXML, StatusBarLog());
				return false;
			}
			TypErrStr parametrToString2 = All.d.GetParametrToString(strXML, "size");
			if (parametrToString2.errCode > 0)
			{
				parametrToString2.ReturnStr = "500";
			}
			int num = (Versioned.IsNumeric((object)parametrToString2.ReturnStr) ? Conversions.ToInteger(parametrToString2.ReturnStr) : 500);
			if (num > 2000)
			{
				num = 2000;
			}
			if (num < 1)
			{
				num = 500;
			}
			All.l.OpenShiftReturn = false;
			Offlin offlin = new Offlin();
			if (!All.l.BlockBase(bl: true))
			{
				All.A.CurrentStatus = "Err=" + 57;
				All.A.ErrHelp = "Превышен лимит времени ожидания доступа к базе";
				All.A.ErrCode = 57;
				All.Lg.SaveTextToLog("GetOfflineNumbers", strXML, StatusBarLog());
				return false;
			}
			TypErr typErr = offlin.NewBackupNumbers(num);
			All.l.BlockBase(bl: false);
			All.l.OpenShiftReturn = true;
			if (typErr.errCode > 0)
			{
				All.A.CurrentStatus = "Err=" + typErr.errCode;
				All.A.ErrHelp = typErr.errStr;
				All.A.ErrCode = typErr.errCode;
				All.Lg.SaveTextToLog("GetOfflineNumbers", strXML, StatusBarLog());
				return false;
			}
			string returnStr = All.l.MaxID("ksef").ReturnStr;
			string strXML2 = "<InputParameters><Parameters ID='" + returnStr + "'/></InputParameters>";
			if (All.A.ShowPrintForm)
			{
				ShowPrintByCheckFn(strXML2);
			}
			NumbersOfflineUse numbersOfflineUse = new NumbersOfflineUse();
			All.A.CurrentStatus = "Err=0_FN=" + All.A.FN + "_OfflinePool=" + numbersOfflineUse.CountNubmers();
			All.A.ErrHelp = "";
			All.A.ErrCode = 0;
			All.Lg.SaveTextToLog("GetOfflineNumbers", strXML, StatusBarLog());
			return true;
		}
		parametrToString.errCode = 4;
		parametrToString.errStr = "Нет подключения";
		All.A.CurrentStatus = "Err=" + parametrToString.errCode;
		All.A.ErrHelp = parametrToString.errStr;
		All.A.ErrCode = parametrToString.errCode;
		All.Lg.SaveTextToLog("GetOfflineNumbers", strXML, StatusBarLog());
		return false;
	}

	public bool OnlineToOffline(string strXML)
	{
		TypErrStr parametrToString = All.d.GetParametrToString(strXML, "fn");
		if (parametrToString.errCode > 0)
		{
			All.A.CurrentStatus = "Err=" + parametrToString.errCode;
			All.A.ErrHelp = parametrToString.errStr;
			All.A.ErrCode = parametrToString.errCode;
			All.Lg.SaveTextToLog("OnlineToOffline", strXML, StatusBarLog());
			return false;
		}
		if (!Versioned.IsNumeric((object)parametrToString.ReturnStr) | (parametrToString.ReturnStr.Length != 10))
		{
			parametrToString.errCode = 3;
			parametrToString.errStr = "Неправильний формат Фіскального Номера: " + parametrToString.ReturnStr;
			All.A.CurrentStatus = "Err=" + parametrToString.errCode;
			All.A.ErrHelp = parametrToString.errStr;
			All.A.ErrCode = parametrToString.errCode;
			All.Lg.SaveTextToLog("OnlineToOffline", strXML, StatusBarLog());
			return false;
		}
		if (Operators.CompareString(All.f.StringGetFn(parametrToString.ReturnStr, "Path"), "", false) == 0)
		{
			parametrToString.errCode = 1001;
			parametrToString.errStr = "Фіскального номера " + parametrToString.ReturnStr + " у базі немає.";
			All.A.CurrentStatus = "Err=" + parametrToString.errCode;
			All.A.ErrCode = parametrToString.errCode;
			All.A.ErrHelp = parametrToString.errStr;
			All.Lg.SaveTextToLog("OnlineToOffline", strXML, StatusBarLog());
			return false;
		}
		if (All.f.IntegerGetFn(parametrToString.ReturnStr, "On") < 1)
		{
			parametrToString.errCode = 1;
			parametrToString.errStr = "Доступ к Фискальному Номеру " + parametrToString.ReturnStr + " запрещен.";
			All.A.CurrentStatus = "Err=" + parametrToString.errCode;
			All.A.ErrHelp = parametrToString.errStr;
			All.A.ErrCode = parametrToString.errCode;
			All.Lg.SaveTextToLog("OnlineToOffline", strXML, StatusBarLog());
			return false;
		}
		if (All.A.Status)
		{
			if (Operators.CompareString(parametrToString.ReturnStr, All.A.FN, false) != 0)
			{
				parametrToString.errCode = 2;
				parametrToString.errStr = "Ранее был подключен другой ФН.";
				All.A.CurrentStatus = "Err=" + parametrToString.errCode;
				All.A.ErrHelp = parametrToString.errStr;
				All.A.ErrCode = parametrToString.errCode;
				All.Lg.SaveTextToLog("OnlineToOffline", strXML, StatusBarLog());
				return false;
			}
			if (!All.A.FullVersion)
			{
				parametrToString.errStr = "В бесплатной версии нет режима оффлайн.";
				parametrToString.errCode = 37;
				All.A.CurrentStatus = "Err=" + parametrToString.errCode;
				All.A.ErrHelp = parametrToString.errStr;
				All.A.ErrCode = parametrToString.errCode;
				All.Lg.SaveTextToLog("OnlineToOffline", strXML, StatusBarLog());
				return false;
			}
			if (All.l.OfflineTrue())
			{
				parametrToString.errStr = "Оффлайн режим включен ранее.";
				parametrToString.errCode = 41;
				All.A.CurrentStatus = "Err=" + parametrToString.errCode;
				All.A.ErrHelp = parametrToString.errStr;
				All.A.ErrCode = parametrToString.errCode;
				All.Lg.SaveTextToLog("OnlineToOffline", strXML, StatusBarLog());
				return false;
			}
			if (!All.OfflineTimeOn())
			{
				parametrToString.errStr = "Перевищено ліміт часу роботи в режимі офлайн (" + All.f.StringGetFn(All.A.FN, "LastOfflineErr") + ")";
				parametrToString.errCode = 42;
				All.A.CurrentStatus = "Err=" + parametrToString.errCode;
				All.A.ErrHelp = parametrToString.errStr;
				All.A.ErrCode = parametrToString.errCode;
				All.Lg.SaveTextToLog("OnlineToOffline", strXML, StatusBarLog());
				return false;
			}
			if (!All.l.BlockBase(bl: true))
			{
				All.A.CurrentStatus = "Err=" + 57;
				All.A.ErrHelp = "Превышен лимит времени ожидания доступа к базе";
				All.A.ErrCode = 57;
				All.Lg.SaveTextToLog("OnlineToOffline", strXML, StatusBarLog());
				return false;
			}
			TypErr typErr = All.OfflineOnManually();
			All.l.BlockBase(bl: false);
			if (typErr.errCode > 0)
			{
				parametrToString.errStr = typErr.errStr;
				parametrToString.errCode = typErr.errCode;
				All.A.CurrentStatus = "Err=" + parametrToString.errCode;
				All.A.ErrHelp = parametrToString.errStr;
				All.A.ErrCode = parametrToString.errCode;
				All.Lg.SaveTextToLog("OnlineToOffline", strXML, StatusBarLog());
				return false;
			}
			string returnStr = All.l.MaxID("ksef").ReturnStr;
			_ = "<InputParameters><Parameters ID='" + returnStr + "'/></InputParameters>";
			_ = All.A.ShowPrintForm;
			TypPrintChecks typPrintChecks = All.Rf.CheckXMLNumber(returnStr, SearchID: true);
			new PrintExportCheck().ExportToFile(typPrintChecks.ReturnStr, typPrintChecks.ReturnStrTaxN);
			All.A.CurrentStatus = "Err=0_FN=" + All.A.FN + "_CheckID=" + All.OfflineNum + "_Offline=" + All.l.OfflineTrueInt();
			All.A.ErrHelp = "";
			All.A.ErrCode = 0;
			All.Lg.SaveTextToLog("ВНИМАНИЕ! Ручной перевод OnlineToOffline", strXML, StatusBarLog());
			return true;
		}
		parametrToString.errCode = 4;
		parametrToString.errStr = "Нет подключения";
		All.A.CurrentStatus = "Err=" + parametrToString.errCode;
		All.A.ErrHelp = parametrToString.errStr;
		All.A.ErrCode = parametrToString.errCode;
		All.Lg.SaveTextToLog("OnlineToOffline", strXML, StatusBarLog());
		return false;
	}

	public bool AddPayForm(string strXML)
	{
		TypErrStr parametrToString = All.d.GetParametrToString(strXML, "fn");
		if (parametrToString.errCode > 0)
		{
			All.A.CurrentStatus = "Err=" + parametrToString.errCode;
			All.A.ErrHelp = parametrToString.errStr;
			All.A.ErrCode = parametrToString.errCode;
			All.Lg.SaveTextToLog("AddPayForm", strXML, StatusBarLog());
			return false;
		}
		if (!Versioned.IsNumeric((object)parametrToString.ReturnStr) | (parametrToString.ReturnStr.Length != 10))
		{
			parametrToString.errCode = 3;
			parametrToString.errStr = "Неправильний формат Фіскального Номера: " + parametrToString.ReturnStr;
			All.A.CurrentStatus = "Err=" + parametrToString.errCode;
			All.A.ErrHelp = parametrToString.errStr;
			All.A.ErrCode = parametrToString.errCode;
			All.Lg.SaveTextToLog("AddPayForm", strXML, StatusBarLog());
			return false;
		}
		if (Operators.CompareString(All.f.StringGetFn(parametrToString.ReturnStr, "Path"), "", false) == 0)
		{
			parametrToString.errCode = 1001;
			parametrToString.errStr = "Фіскального номера " + parametrToString.ReturnStr + " у базі немає.";
			All.A.CurrentStatus = "Err=" + parametrToString.errCode;
			All.A.ErrCode = parametrToString.errCode;
			All.A.ErrHelp = parametrToString.errStr;
			All.Lg.SaveTextToLog("AddPayForm", strXML, StatusBarLog());
			return false;
		}
		if (All.f.IntegerGetFn(parametrToString.ReturnStr, "On") < 1)
		{
			parametrToString.errCode = 1;
			parametrToString.errStr = "Доступ к Фискальному Номеру " + parametrToString.ReturnStr + " запрещен.";
			All.A.CurrentStatus = "Err=" + parametrToString.errCode;
			All.A.ErrHelp = parametrToString.errStr;
			All.A.ErrCode = parametrToString.errCode;
			All.Lg.SaveTextToLog("AddPayForm", strXML, StatusBarLog());
			return false;
		}
		checked
		{
			if (All.A.Status)
			{
				if (Operators.CompareString(parametrToString.ReturnStr, All.A.FN, false) != 0)
				{
					parametrToString.errCode = 2;
					parametrToString.errStr = "Ранее был подключен другой ФН.";
					All.A.CurrentStatus = "Err=" + parametrToString.errCode;
					All.A.ErrHelp = parametrToString.errStr;
					All.A.ErrCode = parametrToString.errCode;
					All.Lg.SaveTextToLog("AddPayForm", strXML, StatusBarLog());
					return false;
				}
				string text = parametrToString.ReturnStr;
				parametrToString = All.d.GetParametrToString(strXML, "payformname");
				if (parametrToString.errCode > 0)
				{
					All.A.CurrentStatus = "Err=" + parametrToString.errCode;
					All.A.ErrHelp = parametrToString.errStr;
					All.A.ErrCode = parametrToString.errCode;
					All.Lg.SaveTextToLog("AddPayForm", strXML, StatusBarLog());
					return false;
				}
				TypErr typErr = All.ForbiddenSymbols(parametrToString.ReturnStr);
				if (typErr.errCode > 0)
				{
					All.A.CurrentStatus = "Err=" + typErr.errCode;
					All.A.ErrHelp = typErr.errStr;
					All.A.ErrCode = typErr.errCode;
					All.Lg.SaveTextToLog("AddPayForm", strXML, StatusBarLog());
					return false;
				}
				string returnStr = parametrToString.ReturnStr;
				string text2 = "";
				int num = returnStr.Length - 1;
				for (int i = 0; i <= num; i++)
				{
					text2 = ((i != 0) ? (text2 + returnStr[i]) : (text2 + returnStr[i].ToString().ToUpper()));
				}
				returnStr = text2.Trim();
				if (Operators.CompareString(All.A.FiscalMode, "cabinet.tax.gov.ua:9443", false) == 0)
				{
					text += "_TS";
				}
				TypErr typErr2 = new CreateDB(text).SavePayForms(returnStr, "0");
				if (typErr2.errCode > 0)
				{
					All.A.CurrentStatus = "Err=" + typErr2.errCode;
					All.A.ErrHelp = typErr2.errStr;
					All.A.ErrCode = typErr2.errCode;
					All.Lg.SaveTextToLog("AddPayForm", strXML, StatusBarLog());
					return false;
				}
				typErr2 = All.PayTax.StartLoad();
				if (typErr2.errCode > 0)
				{
					All.A.CurrentStatus = "Err=" + typErr2.errCode;
					All.A.ErrHelp = typErr2.errStr;
					All.A.ErrCode = typErr2.errCode;
					All.Lg.SaveTextToLog("AddPayForm", strXML, StatusBarLog());
					return false;
				}
				All.A.CurrentStatus = "Err=" + typErr2.errCode + "_NewPayForm=" + text2;
				All.A.ErrHelp = typErr2.errStr;
				All.A.ErrCode = typErr2.errCode;
				All.Lg.SaveTextToLog("AddPayForm", strXML, StatusBarLog());
				return true;
			}
			parametrToString.errCode = 4;
			parametrToString.errStr = "Нет подключения";
			All.A.CurrentStatus = "Err=" + parametrToString.errCode;
			All.A.ErrHelp = parametrToString.errStr;
			All.A.ErrCode = parametrToString.errCode;
			All.Lg.SaveTextToLog("AddPayForm", strXML, StatusBarLog());
			return false;
		}
	}

	public bool UpdatePayFrom(string strXML)
	{
		TypErrStr parametrToString = All.d.GetParametrToString(strXML, "fn");
		if (parametrToString.errCode > 0)
		{
			All.A.CurrentStatus = "Err=" + parametrToString.errCode;
			All.A.ErrHelp = parametrToString.errStr;
			All.A.ErrCode = parametrToString.errCode;
			All.Lg.SaveTextToLog("UpdatePayFrom", strXML, StatusBarLog());
			return false;
		}
		if (!Versioned.IsNumeric((object)parametrToString.ReturnStr) | (parametrToString.ReturnStr.Length != 10))
		{
			parametrToString.errCode = 3;
			parametrToString.errStr = "Неправильний формат Фіскального Номера: " + parametrToString.ReturnStr;
			All.A.CurrentStatus = "Err=" + parametrToString.errCode;
			All.A.ErrHelp = parametrToString.errStr;
			All.A.ErrCode = parametrToString.errCode;
			All.Lg.SaveTextToLog("UpdatePayFrom", strXML, StatusBarLog());
			return false;
		}
		if (Operators.CompareString(All.f.StringGetFn(parametrToString.ReturnStr, "Path"), "", false) == 0)
		{
			parametrToString.errCode = 1001;
			parametrToString.errStr = "Фіскального номера " + parametrToString.ReturnStr + " у базі немає.";
			All.A.CurrentStatus = "Err=" + parametrToString.errCode;
			All.A.ErrCode = parametrToString.errCode;
			All.A.ErrHelp = parametrToString.errStr;
			All.Lg.SaveTextToLog("UpdatePayFrom", strXML, StatusBarLog());
			return false;
		}
		if (All.f.IntegerGetFn(parametrToString.ReturnStr, "On") < 1)
		{
			parametrToString.errCode = 1;
			parametrToString.errStr = "Доступ к Фискальному Номеру " + parametrToString.ReturnStr + " запрещен.";
			All.A.CurrentStatus = "Err=" + parametrToString.errCode;
			All.A.ErrHelp = parametrToString.errStr;
			All.A.ErrCode = parametrToString.errCode;
			All.Lg.SaveTextToLog("UpdatePayFrom", strXML, StatusBarLog());
			return false;
		}
		if (All.A.Status)
		{
			if (Operators.CompareString(parametrToString.ReturnStr, All.A.FN, false) != 0)
			{
				parametrToString.errCode = 2;
				parametrToString.errStr = "Ранее был подключен другой ФН.";
				All.A.CurrentStatus = "Err=" + parametrToString.errCode;
				All.A.ErrHelp = parametrToString.errStr;
				All.A.ErrCode = parametrToString.errCode;
				All.Lg.SaveTextToLog("UpdatePayFrom", strXML, StatusBarLog());
				return false;
			}
			if (Operators.CompareString(All.l.ReturnOpenShift().ReturnStr, "0", false) != 0)
			{
				parametrToString.errCode = 105;
				parametrToString.errStr = "Помилка! Є відкрита зміна.";
				All.A.CurrentStatus = "Err=" + parametrToString.errCode;
				All.A.ErrHelp = parametrToString.errStr;
				All.A.ErrCode = parametrToString.errCode;
				All.Lg.SaveTextToLog("UpdatePayFrom", strXML, StatusBarLog());
				return false;
			}
			parametrToString = All.d.GetParametrToString(strXML, "payformid");
			if (parametrToString.errCode > 0)
			{
				All.A.CurrentStatus = "Err=" + parametrToString.errCode;
				All.A.ErrHelp = parametrToString.errStr;
				All.A.ErrCode = parametrToString.errCode;
				All.Lg.SaveTextToLog("UpdatePayFrom", strXML, StatusBarLog());
				return false;
			}
			if (!Versioned.IsNumeric((object)parametrToString.ReturnStr))
			{
				All.A.ErrHelp = "ID платежа должен быть числом";
				All.A.ErrCode = 78;
				All.A.CurrentStatus = "Err=" + All.A.ErrCode;
				All.Lg.SaveTextToLog("UpdatePayFrom", strXML, StatusBarLog());
				return false;
			}
			int num = Conversions.ToInteger(parametrToString.ReturnStr);
			if (num < 5)
			{
				All.A.ErrHelp = "Нельзя переименовать встроенный платеж";
				All.A.ErrCode = 78;
				All.A.CurrentStatus = "Err=" + All.A.ErrCode;
				All.Lg.SaveTextToLog("UpdatePayFrom", strXML, StatusBarLog());
				return false;
			}
			parametrToString = All.d.GetParametrToString(strXML, "payformname");
			TypErr typErr = All.ForbiddenSymbols(parametrToString.ReturnStr);
			if (typErr.errCode > 0)
			{
				All.A.CurrentStatus = "Err=" + typErr.errCode;
				All.A.ErrHelp = typErr.errStr;
				All.A.ErrCode = typErr.errCode;
				All.Lg.SaveTextToLog("AddPayForm", strXML, StatusBarLog());
				return false;
			}
			if (Operators.CompareString(parametrToString.ReturnStr.Trim(), "", false) == 0)
			{
				parametrToString = All.d.GetParametrToString(strXML, "t");
				if (Operators.CompareString(parametrToString.ReturnStr, "", false) == 0)
				{
					All.A.ErrHelp = "Вкажіть правильне ім'я або групу платежу";
					All.A.ErrCode = 78;
					All.A.CurrentStatus = "Err=" + All.A.ErrCode;
					All.Lg.SaveTextToLog("UpdatePayFrom", strXML, StatusBarLog());
					return false;
				}
				string text = parametrToString.ReturnStr.Trim();
				if (!Versioned.IsNumeric((object)text))
				{
					text = "0";
				}
				if ((Conversions.ToInteger(text) < 1) | (Conversions.ToInteger(text) > 2))
				{
					All.A.ErrHelp = "Вкажіть правильний номер групи для платежу";
					All.A.ErrCode = 78;
					All.A.CurrentStatus = "Err=" + All.A.ErrCode;
					All.Lg.SaveTextToLog("UpdatePayFrom", strXML, StatusBarLog());
					return false;
				}
				if (Conversions.ToInteger(text) == 2)
				{
					text = "102";
				}
				TypErr typErr2 = new UpdateInfa().UPDATE("PayForms", "ISCASH", num.ToString(), text);
				if (typErr2.errCode > 0)
				{
					All.A.CurrentStatus = "Err=" + typErr2.errCode;
					All.A.ErrHelp = typErr2.errStr;
					All.A.ErrCode = typErr2.errCode;
					All.Lg.SaveTextToLog("UpdatePayFrom", strXML, StatusBarLog());
					return false;
				}
				All.A.CurrentStatus = "Err=" + typErr2.errCode + "_UpdatePayGroup=" + text;
				All.A.ErrHelp = typErr2.errStr;
				All.A.ErrCode = typErr2.errCode;
				All.Lg.SaveTextToLog("UpdatePayFrom", strXML, StatusBarLog());
				return true;
			}
			string text2 = parametrToString.ReturnStr.Trim();
			UpdateInfa updateInfa = new UpdateInfa();
			if (updateInfa.SearchPayForms(text2))
			{
				All.A.ErrHelp = "Такой платеж уже есть";
				All.A.ErrCode = 78;
				All.A.CurrentStatus = "Err=" + All.A.ErrCode;
				All.Lg.SaveTextToLog("UpdatePayFrom", strXML, StatusBarLog());
				return false;
			}
			TypErr typErr3 = updateInfa.UPDATE("PayForms", "NAME", num.ToString(), text2);
			if (typErr3.errCode > 0)
			{
				All.A.CurrentStatus = "Err=" + typErr3.errCode;
				All.A.ErrHelp = typErr3.errStr;
				All.A.ErrCode = typErr3.errCode;
				All.Lg.SaveTextToLog("UpdatePayFrom", strXML, StatusBarLog());
				return false;
			}
			typErr3 = All.PayTax.StartLoad();
			if (typErr3.errCode > 0)
			{
				All.A.CurrentStatus = "Err=" + typErr3.errCode;
				All.A.ErrHelp = typErr3.errStr;
				All.A.ErrCode = typErr3.errCode;
				All.Lg.SaveTextToLog("UpdatePayFrom", strXML, StatusBarLog());
				return false;
			}
			All.A.CurrentStatus = "Err=" + typErr3.errCode + "_UpdatePayFrom=" + text2;
			All.A.ErrHelp = typErr3.errStr;
			All.A.ErrCode = typErr3.errCode;
			All.Lg.SaveTextToLog("UpdatePayFrom", strXML, StatusBarLog());
			return true;
		}
		parametrToString.errCode = 4;
		parametrToString.errStr = "Нет подключения";
		All.A.CurrentStatus = "Err=" + parametrToString.errCode;
		All.A.ErrHelp = parametrToString.errStr;
		All.A.ErrCode = parametrToString.errCode;
		All.Lg.SaveTextToLog("UpdatePayFrom", strXML, StatusBarLog());
		return false;
	}

	public bool AddOperator(string strXML)
	{
		TypErrStr parametrToString = All.d.GetParametrToString(strXML, "fn");
		if (parametrToString.errCode > 0)
		{
			All.A.CurrentStatus = "Err=" + parametrToString.errCode;
			All.A.ErrHelp = parametrToString.errStr;
			All.A.ErrCode = parametrToString.errCode;
			All.Lg.SaveTextToLog("AddOperator", strXML, StatusBarLog());
			return false;
		}
		if (!Versioned.IsNumeric((object)parametrToString.ReturnStr) | (parametrToString.ReturnStr.Length != 10))
		{
			parametrToString.errCode = 3;
			parametrToString.errStr = "Неправильний формат Фіскального Номера: " + parametrToString.ReturnStr;
			All.A.CurrentStatus = "Err=" + parametrToString.errCode;
			All.A.ErrHelp = parametrToString.errStr;
			All.A.ErrCode = parametrToString.errCode;
			All.Lg.SaveTextToLog("AddOperator", strXML, StatusBarLog());
			return false;
		}
		if (Operators.CompareString(All.f.StringGetFn(parametrToString.ReturnStr, "Path"), "", false) == 0)
		{
			parametrToString.errCode = 1001;
			parametrToString.errStr = "Фіскального номера " + parametrToString.ReturnStr + " у базі немає.";
			All.A.CurrentStatus = "Err=" + parametrToString.errCode;
			All.A.ErrCode = parametrToString.errCode;
			All.A.ErrHelp = parametrToString.errStr;
			All.Lg.SaveTextToLog("AddOperator", strXML, StatusBarLog());
			return false;
		}
		if (All.f.IntegerGetFn(parametrToString.ReturnStr, "On") < 1)
		{
			parametrToString.errCode = 1;
			parametrToString.errStr = "Доступ к Фискальному Номеру " + parametrToString.ReturnStr + " запрещен.";
			All.A.CurrentStatus = "Err=" + parametrToString.errCode;
			All.A.ErrHelp = parametrToString.errStr;
			All.A.ErrCode = parametrToString.errCode;
			All.Lg.SaveTextToLog("AddOperator", strXML, StatusBarLog());
			return false;
		}
		if (All.A.Status)
		{
			if (Operators.CompareString(parametrToString.ReturnStr, All.A.FN, false) != 0)
			{
				parametrToString.errCode = 2;
				parametrToString.errStr = "Ранее был подключен другой ФН.";
				All.A.CurrentStatus = "Err=" + parametrToString.errCode;
				All.A.ErrHelp = parametrToString.errStr;
				All.A.ErrCode = parametrToString.errCode;
				All.Lg.SaveTextToLog("AddOperator", strXML, StatusBarLog());
				return false;
			}
			strXML = All.d.TegXml(strXML);
			parametrToString = All.d.GetParametrToString(strXML, "operatorname", "InputParameters/Parameters", RegUpLow: true);
			if (parametrToString.errCode > 0)
			{
				All.A.CurrentStatus = "Err=" + parametrToString.errCode;
				All.A.ErrHelp = parametrToString.errStr;
				All.A.ErrCode = parametrToString.errCode;
				All.Lg.SaveTextToLog("AddOperator", strXML, StatusBarLog());
				return false;
			}
			string text = parametrToString.ReturnStr.Trim();
			if (Operators.CompareString(text.Trim(), "", false) == 0)
			{
				All.A.ErrCode = 83;
				All.A.ErrHelp = "Укажите имя оператора";
				All.A.CurrentStatus = "Err=" + parametrToString.errCode;
				All.Lg.SaveTextToLog("AddOperator", strXML, StatusBarLog());
				return false;
			}
			parametrToString = All.d.GetParametrToString(strXML, "keypath", "InputParameters/Parameters", RegUpLow: true);
			if (parametrToString.errCode > 0)
			{
				All.A.CurrentStatus = "Err=" + parametrToString.errCode;
				All.A.ErrHelp = parametrToString.errStr;
				All.A.ErrCode = parametrToString.errCode;
				All.Lg.SaveTextToLog("AddOperator", strXML, StatusBarLog());
				return false;
			}
			string text2 = parametrToString.ReturnStr.Trim();
			if (Operators.CompareString(text2.Trim(), "", false) == 0)
			{
				All.A.ErrCode = 83;
				All.A.ErrHelp = "Укажите путь к файлу ключа";
				All.A.CurrentStatus = "Err=" + parametrToString.errCode;
				All.Lg.SaveTextToLog("AddOperator", strXML, StatusBarLog());
				return false;
			}
			parametrToString = All.d.GetParametrToString(strXML, "keypass", "InputParameters/Parameters", RegUpLow: true);
			if (parametrToString.errCode > 0)
			{
				All.A.CurrentStatus = "Err=" + parametrToString.errCode;
				All.A.ErrHelp = parametrToString.errStr;
				All.A.ErrCode = parametrToString.errCode;
				All.Lg.SaveTextToLog("AddOperator", strXML, StatusBarLog());
				return false;
			}
			string text3 = parametrToString.ReturnStr.Trim();
			if (Operators.CompareString(text3.Trim(), "", false) == 0)
			{
				All.A.ErrCode = 83;
				All.A.ErrHelp = "Укажите пароль к ключу";
				All.A.CurrentStatus = "Err=" + parametrToString.errCode;
				All.Lg.SaveTextToLog("AddOperator", strXML, StatusBarLog());
				return false;
			}
			parametrToString = All.d.GetParametrToString(strXML, "inn", "InputParameters/Parameters", RegUpLow: true);
			if (parametrToString.errCode > 0)
			{
				All.A.CurrentStatus = "Err=" + parametrToString.errCode;
				All.A.ErrHelp = parametrToString.errStr;
				All.A.ErrCode = parametrToString.errCode;
				All.Lg.SaveTextToLog("AddOperator", strXML, StatusBarLog());
				return false;
			}
			string text4 = parametrToString.ReturnStr.Trim();
			if (Operators.CompareString(text4.Trim(), "", false) == 0)
			{
				All.A.ErrCode = 83;
				All.A.ErrHelp = "Укажите INN оператора";
				All.A.CurrentStatus = "Err=" + parametrToString.errCode;
				All.Lg.SaveTextToLog("AddOperator", strXML, StatusBarLog());
				return false;
			}
			if (new OperatorsAll().CountOperators(text4) > 0)
			{
				All.A.ErrHelp = "Оператор с таким ИНН уже есть";
				All.A.ErrCode = 83;
				All.A.CurrentStatus = "Err=" + All.A.ErrCode;
				All.Lg.SaveTextToLog("AddOperator", strXML, StatusBarLog());
				return false;
			}
			if (!All.l.AddNewOperator(All.l.TextToTextXML(text), text2, text3, text4))
			{
				All.A.ErrHelp = "Ошибка записи нового оператора в базу";
				All.A.ErrCode = 83;
				All.A.CurrentStatus = "Err=" + All.A.ErrCode;
				All.Lg.SaveTextToLog("AddOperator", strXML, StatusBarLog());
				return false;
			}
			All.A.CurrentStatus = "Err=0_AddNewOperator=" + text + "_INN=" + text4;
			All.A.ErrHelp = "";
			All.A.ErrCode = 0;
			All.Lg.SaveTextToLog("AddOperator", strXML, StatusBarLog());
			return true;
		}
		parametrToString.errCode = 4;
		parametrToString.errStr = "Нет подключения";
		All.A.CurrentStatus = "Err=" + parametrToString.errCode;
		All.A.ErrHelp = parametrToString.errStr;
		All.A.ErrCode = parametrToString.errCode;
		All.Lg.SaveTextToLog("AddOperator", strXML, StatusBarLog());
		return false;
	}

	public bool AddPRRO(string strXML)
	{
		TypErrStr parametrToString = All.d.GetParametrToString(strXML, "fn");
		if (parametrToString.errCode > 0)
		{
			All.A.CurrentStatus = "Err=" + parametrToString.errCode;
			All.A.ErrHelp = parametrToString.errStr;
			All.A.ErrCode = parametrToString.errCode;
			All.Lg.SaveTextToLog("AddPRRO", strXML, StatusBarLog());
			return false;
		}
		if (!Versioned.IsNumeric((object)parametrToString.ReturnStr) | (parametrToString.ReturnStr.Length != 10))
		{
			parametrToString.errCode = 85;
			parametrToString.errStr = "Неправильний формат Фіскального Номера: " + parametrToString.ReturnStr;
			All.A.CurrentStatus = "Err=" + parametrToString.errCode;
			All.A.ErrHelp = parametrToString.errStr;
			All.A.ErrCode = parametrToString.errCode;
			All.Lg.SaveTextToLog("AddPRRO", strXML, StatusBarLog());
			return false;
		}
		string text = parametrToString.ReturnStr.Trim();
		strXML = All.d.TegXml(strXML);
		parametrToString = All.d.GetParametrToString(strXML, "tin", "InputParameters/Parameters", RegUpLow: true);
		if (parametrToString.errCode > 0)
		{
			All.A.CurrentStatus = "Err=" + parametrToString.errCode;
			All.A.ErrHelp = parametrToString.errStr;
			All.A.ErrCode = parametrToString.errCode;
			All.Lg.SaveTextToLog("AddPRRO", strXML, StatusBarLog());
			return false;
		}
		if (Operators.CompareString(parametrToString.ReturnStr.Trim(), "", false) == 0)
		{
			parametrToString.errCode = 85;
			parametrToString.errStr = "Укажите TIN";
			All.A.CurrentStatus = "Err=" + parametrToString.errCode;
			All.A.ErrHelp = parametrToString.errStr;
			All.A.ErrCode = parametrToString.errCode;
			All.Lg.SaveTextToLog("AddPRRO", strXML, StatusBarLog());
			return false;
		}
		string text2 = parametrToString.ReturnStr.Trim();
		parametrToString = All.d.GetParametrToString(strXML, "acsk");
		if (parametrToString.errCode > 0)
		{
			All.A.CurrentStatus = "Err=" + parametrToString.errCode;
			All.A.ErrHelp = parametrToString.errStr;
			All.A.ErrCode = parametrToString.errCode;
			All.Lg.SaveTextToLog("AddPRRO", strXML, StatusBarLog());
			return false;
		}
		if (!Versioned.IsNumeric((object)parametrToString.ReturnStr.Trim()))
		{
			parametrToString.errCode = 85;
			parametrToString.errStr = "Неправильный формат ACSK";
			All.A.CurrentStatus = "Err=" + parametrToString.errCode;
			All.A.ErrHelp = parametrToString.errStr;
			All.A.ErrCode = parametrToString.errCode;
			All.Lg.SaveTextToLog("AddPRRO", strXML, StatusBarLog());
			return false;
		}
		string val = parametrToString.ReturnStr.Trim();
		parametrToString = All.d.GetParametrToString(strXML, "inn", "InputParameters/Parameters", RegUpLow: true);
		if (parametrToString.errCode > 0)
		{
			All.A.CurrentStatus = "Err=" + parametrToString.errCode;
			All.A.ErrHelp = parametrToString.errStr;
			All.A.ErrCode = parametrToString.errCode;
			All.Lg.SaveTextToLog("AddPRRO", strXML, StatusBarLog());
			return false;
		}
		if (Operators.CompareString(parametrToString.ReturnStr.Trim(), "", false) == 0)
		{
			parametrToString.errCode = 85;
			parametrToString.errStr = "Укажите INN";
			All.A.CurrentStatus = "Err=" + parametrToString.errCode;
			All.A.ErrHelp = parametrToString.errStr;
			All.A.ErrCode = parametrToString.errCode;
			All.Lg.SaveTextToLog("AddPRRO", strXML, StatusBarLog());
			return false;
		}
		string innT = parametrToString.ReturnStr.Trim();
		parametrToString = All.d.GetParametrToString(strXML, "pointname", "InputParameters/Parameters", RegUpLow: true);
		if (parametrToString.errCode > 0)
		{
			All.A.CurrentStatus = "Err=" + parametrToString.errCode;
			All.A.ErrHelp = parametrToString.errStr;
			All.A.ErrCode = parametrToString.errCode;
			All.Lg.SaveTextToLog("AddPRRO", strXML, StatusBarLog());
			return false;
		}
		if (Operators.CompareString(parametrToString.ReturnStr.Trim(), "", false) == 0)
		{
			parametrToString.errCode = 85;
			parametrToString.errStr = "Укажите Название торговой точки";
			All.A.CurrentStatus = "Err=" + parametrToString.errCode;
			All.A.ErrHelp = parametrToString.errStr;
			All.A.ErrCode = parametrToString.errCode;
			All.Lg.SaveTextToLog("AddPRRO", strXML, StatusBarLog());
			return false;
		}
		string ntorgT = parametrToString.ReturnStr.Trim();
		parametrToString = All.d.GetParametrToString(strXML, "orgname", "InputParameters/Parameters", RegUpLow: true);
		if (parametrToString.errCode > 0)
		{
			All.A.CurrentStatus = "Err=" + parametrToString.errCode;
			All.A.ErrHelp = parametrToString.errStr;
			All.A.ErrCode = parametrToString.errCode;
			All.Lg.SaveTextToLog("AddPRRO", strXML, StatusBarLog());
			return false;
		}
		if (Operators.CompareString(parametrToString.ReturnStr.Trim(), "", false) == 0)
		{
			parametrToString.errCode = 85;
			parametrToString.errStr = "Укажите название организации.";
			All.A.CurrentStatus = "Err=" + parametrToString.errCode;
			All.A.ErrHelp = parametrToString.errStr;
			All.A.ErrCode = parametrToString.errCode;
			All.Lg.SaveTextToLog("AddPRRO", strXML, StatusBarLog());
			return false;
		}
		string norgT = parametrToString.ReturnStr.Trim();
		parametrToString = All.d.GetParametrToString(strXML, "pointaddr", "InputParameters/Parameters", RegUpLow: true);
		if (parametrToString.errCode > 0)
		{
			All.A.CurrentStatus = "Err=" + parametrToString.errCode;
			All.A.ErrHelp = parametrToString.errStr;
			All.A.ErrCode = parametrToString.errCode;
			All.Lg.SaveTextToLog("AddPRRO", strXML, StatusBarLog());
			return false;
		}
		if (Operators.CompareString(parametrToString.ReturnStr.Trim(), "", false) == 0)
		{
			parametrToString.errCode = 85;
			parametrToString.errStr = "Укажите адрес торговой точки.";
			All.A.CurrentStatus = "Err=" + parametrToString.errCode;
			All.A.ErrHelp = parametrToString.errStr;
			All.A.ErrCode = parametrToString.errCode;
			All.Lg.SaveTextToLog("AddPRRO", strXML, StatusBarLog());
			return false;
		}
		string atorgT = parametrToString.ReturnStr.Trim();
		if (All.f.IndexFn(text) == 0)
		{
			string text3 = All.MyDoc() + "\\WebCheck\\DB\\" + text.Trim() + ".db";
			All.f.AddFn(text);
			All.f.StringWriteFN(text, "Path", text3);
			All.f.StringWriteFN(text, "TIN", text2);
			All.f.StringWriteFN(text, "On", "1");
			All.f.StringWriteFN(text, "Save", "0");
			All.f.StringWriteFN(text, "ShowPintForm", "1");
			All.f.StringWriteFN(text, "LogOn", "1");
			All.f.StringWriteFN(All.A.FN, "Acsksettings", val);
			if (File.Exists(text3))
			{
				parametrToString.errCode = 85;
				parametrToString.errStr = "Файл с базой данных с таким фискальным номером уже существует";
				All.A.CurrentStatus = "Err=" + parametrToString.errCode;
				All.A.ErrHelp = parametrToString.errStr;
				All.A.ErrCode = parametrToString.errCode;
				All.Lg.SaveTextToLog("AddPRRO", strXML, StatusBarLog());
				return false;
			}
			CreateTables(text);
			CreateRow(text, text2, innT, ntorgT, norgT, atorgT);
			All.NewFolderFn();
			return true;
		}
		parametrToString.errCode = 85;
		parametrToString.errStr = "Такой фискальный номер уже есть";
		All.A.CurrentStatus = "Err=" + parametrToString.errCode;
		All.A.ErrHelp = parametrToString.errStr;
		All.A.ErrCode = parametrToString.errCode;
		All.Lg.SaveTextToLog("AddPRRO", strXML, StatusBarLog());
		return false;
	}

	public bool ApplicationVersionUpdate()
	{
		TypErrStrUpdate typErrStrUpdate = All.d.UPloadXML();
		All.A.CurrentStatus = "currentver=" + All.VersionDll() + "_ver=" + typErrStrUpdate.ReturnVer + "_dt=" + typErrStrUpdate.ReturnVerDt + "_uplink=" + typErrStrUpdate.ReturnUplink + "_importantupdate=" + typErrStrUpdate.ReturnImp + "_updateinf=" + typErrStrUpdate.ReturnInf;
		All.A.ErrHelp = typErrStrUpdate.errStr;
		All.A.ErrCode = typErrStrUpdate.errCode;
		All.A.CurrentStatus = "err=" + All.A.ErrCode + "_errinf=" + All.A.ErrHelp + "_" + All.A.CurrentStatus;
		if (typErrStrUpdate.errCode == 0)
		{
			return true;
		}
		return false;
	}

	public TypErrStrUpdate ApplicationVersionUpdateS()
	{
		return All.d.UPloadXML();
	}

	public bool ShowWizardNewPro(string strFN = "")
	{
		//IL_009d: Unknown result type (might be due to invalid IL or missing references)
		TypErrStr parametrToString = All.d.GetParametrToString(strFN, "fn");
		if (parametrToString.errCode > 0)
		{
			parametrToString.ReturnStr = "";
		}
		else if (!Versioned.IsNumeric((object)parametrToString.ReturnStr) | (parametrToString.ReturnStr.Length != 10))
		{
			parametrToString.ReturnStr = "";
		}
		TypErrStr parametrToString2 = All.d.GetParametrToString(strFN, "operatorid");
		if (parametrToString2.errCode > 0)
		{
			parametrToString2.ReturnStr = "";
		}
		FormNewPro formNewPro = new FormNewPro(parametrToString.ReturnStr, parametrToString2.ReturnStr);
		((Form)formNewPro).ShowDialog();
		((Component)(object)formNewPro).Dispose();
		return true;
	}

	public bool ShowWizardNewProDemo()
	{
		//IL_0012: Unknown result type (might be due to invalid IL or missing references)
		FormNewPro formNewPro = new FormNewPro("", "", DemoInfa: false, DemoOnly: true);
		((Form)formNewPro).ShowDialog();
		((Component)(object)formNewPro).Dispose();
		return true;
	}

	public bool ShowLicenseCheck()
	{
		//IL_0006: Unknown result type (might be due to invalid IL or missing references)
		FormKeys formKeys = new FormKeys();
		((Form)formKeys).ShowDialog();
		((Component)(object)formKeys).Dispose();
		return true;
	}

	public bool ShowCertificate()
	{
		//IL_0006: Unknown result type (might be due to invalid IL or missing references)
		FormCertificate formCertificate = new FormCertificate();
		((Form)formCertificate).ShowDialog();
		((Component)(object)formCertificate).Dispose();
		return true;
	}

	public bool ShowOfflineStatus()
	{
		//IL_0012: Unknown result type (might be due to invalid IL or missing references)
		if (All.A.Status)
		{
			FormOfflineStatus formOfflineStatus = new FormOfflineStatus();
			((Form)formOfflineStatus).ShowDialog();
			((Component)(object)formOfflineStatus).Dispose();
			return true;
		}
		return false;
	}

	public bool ShowDataRRO(string strFN = "")
	{
		//IL_00a3: Unknown result type (might be due to invalid IL or missing references)
		//IL_0078: Unknown result type (might be due to invalid IL or missing references)
		if (!All.A.Status)
		{
			if (Operators.CompareString(strFN, "", false) == 0)
			{
				All.A.ErrHelp = "Ошибка. Подключите нужную ФН или укажиете его в аргументе.";
				All.A.ErrCode = 4;
				All.A.CurrentStatus = "Err=" + All.A.ErrCode;
				return false;
			}
			if (All.InitializationNew(strFN))
			{
				FormNewPro formNewPro = new FormNewPro("", "", DemoInfa: true);
				((Form)formNewPro).ShowDialog();
				((Component)(object)formNewPro).Dispose();
				All.FinalizationNew();
				return true;
			}
			return false;
		}
		FormNewPro formNewPro2 = new FormNewPro("", "", DemoInfa: true);
		((Form)formNewPro2).ShowDialog();
		((Component)(object)formNewPro2).Dispose();
		return true;
	}

	public bool ShowReports(string strFN = "")
	{
		//IL_0088: Unknown result type (might be due to invalid IL or missing references)
		//IL_0069: Unknown result type (might be due to invalid IL or missing references)
		if (!All.A.Status)
		{
			if (Operators.CompareString(strFN, "", false) == 0)
			{
				All.A.ErrHelp = "Ошибка. Подключите нужную базу или укажиете ее номер в аргументе.";
				All.A.ErrCode = 4;
				All.A.CurrentStatus = "Err=" + All.A.ErrCode;
				return false;
			}
			if (All.InitializationNew(strFN))
			{
				FormReports formReports = new FormReports();
				((Form)formReports).ShowDialog();
				((Component)(object)formReports).Dispose();
				All.FinalizationNew();
				return true;
			}
			return false;
		}
		FormReports formReports2 = new FormReports();
		((Form)formReports2).ShowDialog();
		((Component)(object)formReports2).Dispose();
		return true;
	}

	public bool ShowReportsDB(string strXML)
	{
		//IL_020d: Unknown result type (might be due to invalid IL or missing references)
		if (All.A.Status)
		{
			All.A.CurrentStatus = "Err=" + 74;
			All.A.ErrHelp = "Для просмотра смен и чеков Initialization не требуется";
			All.A.ErrCode = 74;
			return false;
		}
		All.TestRegion();
		TypErrStr parametrToString = All.d.GetParametrToString(strXML, "fn");
		if (parametrToString.errCode > 0)
		{
			All.A.CurrentStatus = "Err=" + parametrToString.errCode;
			All.A.ErrHelp = parametrToString.errStr;
			All.A.ErrCode = parametrToString.errCode;
			return false;
		}
		All.A.FN = parametrToString.ReturnStr.Trim();
		parametrToString = All.d.GetParametrToString(strXML, "pathdb");
		if (parametrToString.errCode > 0)
		{
			All.A.CurrentStatus = "Err=" + parametrToString.errCode;
			All.A.ErrHelp = parametrToString.errStr;
			All.A.ErrCode = parametrToString.errCode;
			return false;
		}
		if (File.Exists(parametrToString.ReturnStr))
		{
			All.A.Connection = "Data Source=" + parametrToString.ReturnStr + "; Version=3";
			TypErr typErr = All.PayTax.StartLoad();
			if (typErr.errCode > 0)
			{
				All.A.CurrentStatus = "Err=" + parametrToString.errCode;
				All.A.ErrHelp = typErr.errStr;
				All.A.ErrCode = typErr.errCode;
				return false;
			}
			All.A.Status = true;
			FormReports formReports = new FormReports();
			((Form)formReports).ShowDialog();
			((Component)(object)formReports).Dispose();
			All.A.Connection = "";
			All.A.Status = false;
			All.A.CurrentStatus = "Err=0";
			All.A.ErrHelp = "";
			All.A.ErrCode = 0;
			return true;
		}
		All.A.CurrentStatus = "Err=" + 75;
		All.A.ErrHelp = "Ошибка! Нет такого файла.";
		All.A.ErrCode = 75;
		return false;
	}

	public bool ShowOperators(string strFN = "")
	{
		//IL_0088: Unknown result type (might be due to invalid IL or missing references)
		//IL_0069: Unknown result type (might be due to invalid IL or missing references)
		if (!All.A.Status)
		{
			if (Operators.CompareString(strFN, "", false) == 0)
			{
				All.A.ErrHelp = "Ошибка. Подключите нужную базу или укажиете ее номер в аргументе.";
				All.A.ErrCode = 4;
				All.A.CurrentStatus = "Err=" + All.A.ErrCode;
				return false;
			}
			if (All.InitializationNew(strFN))
			{
				FormOperators formOperators = new FormOperators();
				((Form)formOperators).ShowDialog();
				((Component)(object)formOperators).Dispose();
				All.FinalizationNew();
				return true;
			}
			return false;
		}
		FormOperators formOperators2 = new FormOperators();
		((Form)formOperators2).ShowDialog();
		((Component)(object)formOperators2).Dispose();
		return true;
	}

	public bool ShowAddPayForm(string strFN = "")
	{
		//IL_0088: Unknown result type (might be due to invalid IL or missing references)
		//IL_0069: Unknown result type (might be due to invalid IL or missing references)
		if (!All.A.Status)
		{
			if (Operators.CompareString(strFN, "", false) == 0)
			{
				All.A.ErrHelp = "Ошибка. Подключите нужную базу или укажиете ее номер в аргументе.";
				All.A.ErrCode = 4;
				All.A.CurrentStatus = "Err=" + All.A.ErrCode;
				return false;
			}
			if (All.InitializationNew(strFN))
			{
				FormPay formPay = new FormPay();
				((Form)formPay).ShowDialog();
				((Component)(object)formPay).Dispose();
				All.FinalizationNew();
				return true;
			}
			return false;
		}
		FormPay formPay2 = new FormPay();
		((Form)formPay2).ShowDialog();
		((Component)(object)formPay2).Dispose();
		return true;
	}

	public bool ShowPrintByCheckFn(string strXML = "", string XMLx = "")
	{
		//IL_0056: Unknown result type (might be due to invalid IL or missing references)
		if (!All.A.Status)
		{
			All.A.ErrHelp = "Ошибка печати чека. Нет подключенной базы.";
			All.A.ErrCode = 4;
			All.A.CurrentStatus = "Err=" + All.A.ErrCode;
			return false;
		}
		FormPrint formPrint = new FormPrint(strXML, XMLx);
		((Form)formPrint).ShowDialog();
		((Component)(object)formPrint).Dispose();
		return true;
	}

	public bool ShoweMailSettings()
	{
		//IL_0016: Unknown result type (might be due to invalid IL or missing references)
		if (!All.A.Status)
		{
			return false;
		}
		FormMail formMail = new FormMail();
		((Form)formMail).ShowDialog();
		((Component)(object)formMail).Dispose();
		return true;
	}

	public bool ShowSettings()
	{
		//IL_0016: Unknown result type (might be due to invalid IL or missing references)
		if (!All.A.Status)
		{
			return false;
		}
		FormSettings formSettings = new FormSettings();
		((Form)formSettings).ShowDialog();
		((Component)(object)formSettings).Dispose();
		string fN = All.A.FN;
		Finalization("<InputParameters><Parameters FN='" + fN + "'/></InputParameters>");
		return Initialization("<InputParameters><Parameters FN='" + fN + "'/></InputParameters>");
	}

	public bool ShowPeriodReport()
	{
		//IL_0012: Unknown result type (might be due to invalid IL or missing references)
		if (All.A.Status)
		{
			FormDate formDate = new FormDate();
			((Form)formDate).ShowDialog();
			((Component)(object)formDate).Dispose();
			return true;
		}
		return false;
	}

	public bool ShowTestErrors()
	{
		//IL_0012: Unknown result type (might be due to invalid IL or missing references)
		if (All.A.Status)
		{
			FormTestErrors formTestErrors = new FormTestErrors();
			((Form)formTestErrors).ShowDialog();
			((Component)(object)formTestErrors).Dispose();
			return true;
		}
		return false;
	}

	public bool ShowWebchekFooterView()
	{
		//IL_0006: Unknown result type (might be due to invalid IL or missing references)
		FormWebchekFooterView formWebchekFooterView = new FormWebchekFooterView();
		((Form)formWebchekFooterView).ShowDialog();
		((Component)(object)formWebchekFooterView).Dispose();
		return true;
	}

	public bool ShowBrowseBusinesses()
	{
		//IL_0012: Unknown result type (might be due to invalid IL or missing references)
		if (!All.A.Status)
		{
			FormAccountant formAccountant = new FormAccountant();
			((Form)formAccountant).ShowDialog();
			((Component)(object)formAccountant).Dispose();
			return true;
		}
		return false;
	}

	public bool GetCheckByFiscalNumber(string strFN)
	{
		if (!All.A.Status)
		{
			if (All.InitializationNew(strFN))
			{
				TypErrStr parametrToString = All.d.GetParametrToString(strFN, "type");
				int tnk = 0;
				if (parametrToString.errCode > 0)
				{
					tnk = 1;
				}
				else if (Versioned.IsNumeric((object)parametrToString.ReturnStr))
				{
					tnk = Conversions.ToInteger(parametrToString.ReturnStr);
				}
				parametrToString = All.d.GetParametrToString(strFN, "TaxNum", "InputParameters/Parameters", RegUpLow: true);
				if (parametrToString.errCode > 0)
				{
					parametrToString = All.d.GetParametrToString(strFN, "taxnum", "InputParameters/Parameters", RegUpLow: true);
				}
				if (parametrToString.errCode > 0)
				{
					parametrToString = All.d.GetParametrToString(strFN, "Taxnum", "InputParameters/Parameters", RegUpLow: true);
				}
				if (parametrToString.errCode > 0)
				{
					parametrToString = All.d.GetParametrToString(strFN, "TAXNUM", "InputParameters/Parameters", RegUpLow: true);
				}
				if (parametrToString.errCode > 0)
				{
					All.A.CurrentStatus = "FN=" + All.A.FN + "_Err=" + parametrToString.errCode;
					All.A.ErrHelp = parametrToString.errStr;
					All.A.ErrCode = parametrToString.errCode;
					All.FinalizationNew();
					return false;
				}
				TypErrStr typErrStr = All.Rf.Reprt5(parametrToString.ReturnStr, tnk);
				if (typErrStr.errCode > 0)
				{
					All.A.CurrentStatus = "FN=" + All.A.FN + "_Err=" + typErrStr.errCode;
					All.A.ErrHelp = typErrStr.errStr;
					All.A.ErrCode = typErrStr.errCode;
					All.FinalizationNew();
					return false;
				}
				TypErrStr typErrStr2 = All.Rf.Reprt5(parametrToString.ReturnStr, 8);
				if (typErrStr2.errCode > 0)
				{
					typErrStr2.ReturnStr = "";
				}
				TypErrStr typErrStr3 = All.Rf.Reprt5(parametrToString.ReturnStr, 4);
				if (typErrStr3.errCode > 0)
				{
					typErrStr3.ReturnStr = "0";
				}
				string text = "<MAC ID='" + typErrStr3.ReturnStr + "'>" + typErrStr2.ReturnStr + "</MAC>";
				All.FinalizationNew();
				All.A.Report = Strings.Replace(typErrStr.ReturnStr, "mmmaaaccc", text, 1, -1, (CompareMethod)0);
				All.A.CurrentStatus = "FN=" + All.A.FN + "_Err=" + typErrStr.errCode;
				All.A.ErrHelp = typErrStr.errStr;
				All.A.ErrCode = typErrStr.errCode;
				return true;
			}
			return false;
		}
		TypErrStr parametrToString2 = All.d.GetParametrToString(strFN, "type");
		int num = 0;
		num = ((parametrToString2.errCode > 0) ? 1 : ((!Versioned.IsNumeric((object)parametrToString2.ReturnStr)) ? 1 : Conversions.ToInteger(parametrToString2.ReturnStr)));
		parametrToString2 = All.d.GetParametrToString(strFN, "TaxNum", "InputParameters/Parameters", RegUpLow: true);
		if (parametrToString2.errCode > 0)
		{
			parametrToString2 = All.d.GetParametrToString(strFN, "taxnum", "InputParameters/Parameters", RegUpLow: true);
		}
		if (parametrToString2.errCode > 0)
		{
			parametrToString2 = All.d.GetParametrToString(strFN, "Taxnum", "InputParameters/Parameters", RegUpLow: true);
		}
		if (parametrToString2.errCode > 0)
		{
			parametrToString2 = All.d.GetParametrToString(strFN, "TAXNUM", "InputParameters/Parameters", RegUpLow: true);
		}
		if (parametrToString2.errCode > 0)
		{
			All.A.CurrentStatus = "FN=" + All.A.FN + "_Err=" + parametrToString2.errCode;
			All.A.ErrHelp = parametrToString2.errStr;
			All.A.ErrCode = parametrToString2.errCode;
			return false;
		}
		TypErrStr typErrStr4 = All.Rf.Reprt5(parametrToString2.ReturnStr, num);
		if (typErrStr4.errCode > 0)
		{
			All.A.CurrentStatus = "FN=" + All.A.FN + "_Err=" + typErrStr4.errCode;
			All.A.ErrHelp = typErrStr4.errStr;
			All.A.ErrCode = typErrStr4.errCode;
			return false;
		}
		TypErrStr typErrStr5 = All.Rf.Reprt5(parametrToString2.ReturnStr, 8);
		if (typErrStr5.errCode > 0)
		{
			typErrStr5.ReturnStr = "";
		}
		TypErrStr typErrStr6 = All.Rf.Reprt5(parametrToString2.ReturnStr, 4);
		if (typErrStr6.errCode > 0)
		{
			typErrStr6.ReturnStr = "0";
		}
		string text2 = "<MAC ID='" + typErrStr6.ReturnStr + "'>" + typErrStr5.ReturnStr + "</MAC>";
		All.A.Report = Strings.Replace(typErrStr4.ReturnStr, "mmmaaaccc", text2, 1, -1, (CompareMethod)0);
		All.A.CurrentStatus = "FN=" + All.A.FN + "_Err=" + typErrStr4.errCode;
		All.A.ErrHelp = typErrStr4.errStr;
		All.A.ErrCode = typErrStr4.errCode;
		return true;
	}

	public bool GetSetingsRRO(string strFN)
	{
		TypErrStr typErrStr = default(TypErrStr);
		typErrStr.errCode = 0;
		typErrStr.errStr = "";
		typErrStr.ReturnStr = "";
		if (!All.A.Status)
		{
			if (!All.InitializationNew(strFN))
			{
				return false;
			}
			typErrStr = All.d.FulStatusPro(strFN);
			if (typErrStr.errCode > 0)
			{
				All.A.Report = "";
				All.A.CurrentStatus = "FN=" + All.A.FN + "_Err=" + typErrStr.errCode;
				All.A.ErrHelp = typErrStr.errStr;
				All.A.ErrCode = typErrStr.errCode;
				All.FinalizationNew();
				return false;
			}
			All.FinalizationNew();
		}
		else
		{
			typErrStr = All.d.FulStatusPro(strFN);
			if (typErrStr.errCode > 0)
			{
				All.A.Report = "";
				All.A.CurrentStatus = "FN=" + All.A.FN + "_Err=" + typErrStr.errCode;
				All.A.ErrHelp = typErrStr.errStr;
				All.A.ErrCode = typErrStr.errCode;
				All.FinalizationNew();
				return false;
			}
		}
		All.A.Report = typErrStr.ReturnStr;
		return true;
	}

	public bool GetDocumentsByShift(string strFN)
	{
		TypErr typErr = default(TypErr);
		typErr.errCode = 0;
		typErr.errStr = "";
		All.A.Report = "";
		TypErrStr parametrToString = All.d.GetParametrToString(strFN, "fn");
		if (parametrToString.errCode > 0)
		{
			All.A.CurrentStatus = "Err=" + parametrToString.errCode;
			All.A.ErrHelp = parametrToString.errStr;
			All.A.ErrCode = parametrToString.errCode;
			All.Lg.SaveTextToLog("GetDocumentsByShift", strFN, StatusBarLog());
			return false;
		}
		parametrToString = All.d.GetParametrToString(strFN, "ShiftId");
		if (parametrToString.errCode > 0)
		{
			All.A.CurrentStatus = "Err=" + parametrToString.errCode;
			All.A.ErrHelp = parametrToString.errStr;
			All.A.ErrCode = parametrToString.errCode;
			All.Lg.SaveTextToLog("GetDocumentsByShift", strFN, StatusBarLog());
			return false;
		}
		if (!Versioned.IsNumeric((object)parametrToString.ReturnStr))
		{
			All.A.ErrHelp = "Ошибка. Укажите правильно номер смены.";
			All.A.ErrCode = 79;
			All.A.CurrentStatus = "Err=" + All.A.ErrCode;
			All.Lg.SaveTextToLog("GetDocumentsByShift", strFN, StatusBarLog());
			return false;
		}
		int shiftID = Conversions.ToInteger(parametrToString.ReturnStr);
		string text = "";
		if (!All.A.Status)
		{
			if (!All.InitializationNew(strFN))
			{
				All.A.ErrHelp = "Ошибка подключения к указанному ФН.";
				All.A.ErrCode = 80;
				All.A.CurrentStatus = "Err=" + All.A.ErrCode;
				All.Lg.SaveTextToLog("GetDocumentsByShift", strFN, StatusBarLog());
				return false;
			}
			text = new ForServer().GetDocumentsByShifts(shiftID);
			All.FinalizationNew();
		}
		else
		{
			text = new ForServer().GetDocumentsByShifts(shiftID);
		}
		if (Operators.CompareString(text.Trim(), "", false) != 0)
		{
			All.A.CurrentStatus = "FN=" + All.A.FN;
			All.A.ErrHelp = "";
			All.A.ErrCode = 0;
			All.A.Report = text;
			return true;
		}
		All.A.ErrHelp = "Ошибка при получении списка документов в смене";
		All.A.ErrCode = 81;
		All.A.CurrentStatus = "FN=" + All.A.FN + "_Err=" + All.A.ErrCode;
		All.A.Report = "";
		All.Lg.SaveTextToLog("GetDocumentsByShift", strFN, StatusBarLog());
		return false;
	}

	public bool GetShifts(string strFN)
	{
		TypErrStr typErrStr = default(TypErrStr);
		typErrStr.errCode = 0;
		typErrStr.errStr = "";
		typErrStr.ReturnStr = "";
		All.A.Report = "";
		typErrStr = All.d.GetParametrToString(strFN, "fn");
		if (typErrStr.errCode > 0)
		{
			All.A.CurrentStatus = "Err=" + typErrStr.errCode;
			All.A.ErrHelp = typErrStr.errStr;
			All.A.ErrCode = typErrStr.errCode;
			All.Lg.SaveTextToLog("GetShifts", strFN, StatusBarLog());
			return false;
		}
		typErrStr = All.d.GetParametrToString(strFN, "ShiftMonth");
		if (typErrStr.errCode > 0)
		{
			All.A.CurrentStatus = "Err=" + typErrStr.errCode;
			All.A.ErrHelp = typErrStr.errStr;
			All.A.ErrCode = typErrStr.errCode;
			All.Lg.SaveTextToLog("GetShifts", strFN, StatusBarLog());
			return false;
		}
		string returnStr = typErrStr.ReturnStr;
		string text = "";
		if (!All.A.Status)
		{
			if (!All.InitializationNew(strFN))
			{
				All.A.ErrHelp = "Ошибка подключения к указанному ФН.";
				All.A.ErrCode = 80;
				All.A.CurrentStatus = "Err=" + All.A.ErrCode;
				All.Lg.SaveTextToLog("GetShifts", strFN, StatusBarLog());
				return false;
			}
			text = new ForServer().GetShiftsDate(returnStr);
			All.FinalizationNew();
		}
		else
		{
			text = new ForServer().GetShiftsDate(returnStr);
		}
		if (Operators.CompareString(text.Trim(), "", false) != 0)
		{
			All.A.CurrentStatus = "FN=" + All.A.FN;
			All.A.ErrHelp = "";
			All.A.ErrCode = 0;
			All.A.Report = text;
			return true;
		}
		All.A.ErrHelp = "Ошибка при получении списка смен";
		All.A.ErrCode = 82;
		All.A.CurrentStatus = "FN=" + All.A.FN + "_Err=" + All.A.ErrCode;
		All.A.Report = "";
		All.Lg.SaveTextToLog("GetShifts", strFN, StatusBarLog());
		return false;
	}

	public bool GetPeriodReport(string strXML)
	{
		TypErrStr parametrToString = All.d.GetParametrToString(strXML, "fn");
		if (parametrToString.errCode > 0)
		{
			All.A.CurrentStatus = "Err=" + parametrToString.errCode;
			All.A.ErrHelp = parametrToString.errStr;
			All.A.ErrCode = parametrToString.errCode;
			All.Lg.SaveTextToLog("GetPeriodReport", strXML, StatusBarLog());
			return false;
		}
		if (!Versioned.IsNumeric((object)parametrToString.ReturnStr) | (parametrToString.ReturnStr.Length != 10))
		{
			parametrToString.errCode = 3;
			parametrToString.errStr = "Неправильний формат Фіскального Номера: " + parametrToString.ReturnStr;
			All.A.CurrentStatus = "Err=" + parametrToString.errCode;
			All.A.ErrHelp = parametrToString.errStr;
			All.A.ErrCode = parametrToString.errCode;
			All.Lg.SaveTextToLog("GetPeriodReport", strXML, StatusBarLog());
			return false;
		}
		if (Operators.CompareString(All.f.StringGetFn(parametrToString.ReturnStr, "Path"), "", false) == 0)
		{
			parametrToString.errCode = 1001;
			parametrToString.errStr = "Фіскального номера " + parametrToString.ReturnStr + " у базі немає.";
			All.A.CurrentStatus = "Err=" + parametrToString.errCode;
			All.A.ErrCode = parametrToString.errCode;
			All.A.ErrHelp = parametrToString.errStr;
			All.Lg.SaveTextToLog("GetPeriodReport", strXML, StatusBarLog());
			return false;
		}
		if (All.f.IntegerGetFn(parametrToString.ReturnStr, "On") < 1)
		{
			parametrToString.errCode = 1;
			parametrToString.errStr = "Доступ к Фискальному Номеру " + parametrToString.ReturnStr + " запрещен.";
			All.A.CurrentStatus = "Err=" + parametrToString.errCode;
			All.A.ErrHelp = parametrToString.errStr;
			All.A.ErrCode = parametrToString.errCode;
			All.Lg.SaveTextToLog("GetPeriodReport", strXML, StatusBarLog());
			return false;
		}
		if (All.A.Status)
		{
			if (Operators.CompareString(parametrToString.ReturnStr, All.A.FN, false) != 0)
			{
				parametrToString.errCode = 2;
				parametrToString.errStr = "Ранее был подключен другой ФН.";
				All.A.CurrentStatus = "Err=" + parametrToString.errCode;
				All.A.ErrHelp = parametrToString.errStr;
				All.A.ErrCode = parametrToString.errCode;
				All.Lg.SaveTextToLog("GetPeriodReport", strXML, StatusBarLog());
				return false;
			}
			TypErrStr typErrStr = new DateReport().PeriodR(strXML);
			if (typErrStr.errCode > 0)
			{
				All.A.CurrentStatus = "Err=" + typErrStr.errCode;
				All.A.ErrHelp = typErrStr.errStr;
				All.A.ErrCode = typErrStr.errCode;
				All.Lg.SaveTextToLog("GetPeriodReport", strXML, StatusBarLog());
				return false;
			}
			All.A.Report = typErrStr.ReturnStr;
			All.A.CurrentStatus = "Err=0_FN=" + All.A.FN;
			All.A.ErrHelp = "";
			All.A.ErrCode = 0;
			All.Lg.SaveTextToLog("GetPeriodReport", strXML, StatusBarLog());
			return true;
		}
		parametrToString.errCode = 4;
		parametrToString.errStr = "Нет подключения";
		All.A.CurrentStatus = "Err=" + parametrToString.errCode;
		All.A.ErrHelp = parametrToString.errStr;
		All.A.ErrCode = parametrToString.errCode;
		All.Lg.SaveTextToLog("GetPeriodReport", strXML, StatusBarLog());
		return false;
	}

	public bool GetCheckFNbyUID(string strXML)
	{
		TypErrStr parametrToString = All.d.GetParametrToString(strXML, "fn");
		if (parametrToString.errCode > 0)
		{
			All.A.CurrentStatus = "Err=" + parametrToString.errCode;
			All.A.ErrHelp = parametrToString.errStr;
			All.A.ErrCode = parametrToString.errCode;
			All.Lg.SaveTextToLog("GetCheckFNbyUID", strXML, StatusBarLog());
			return false;
		}
		if (!Versioned.IsNumeric((object)parametrToString.ReturnStr) | (parametrToString.ReturnStr.Length != 10))
		{
			parametrToString.errCode = 3;
			parametrToString.errStr = "Неправильний формат Фіскального Номера: " + parametrToString.ReturnStr;
			All.A.CurrentStatus = "Err=" + parametrToString.errCode;
			All.A.ErrHelp = parametrToString.errStr;
			All.A.ErrCode = parametrToString.errCode;
			All.Lg.SaveTextToLog("GetCheckFNbyUID", strXML, StatusBarLog());
			return false;
		}
		if (Operators.CompareString(All.f.StringGetFn(parametrToString.ReturnStr, "Path"), "", false) == 0)
		{
			parametrToString.errCode = 1001;
			parametrToString.errStr = "Фіскального номера " + parametrToString.ReturnStr + " у базі немає.";
			All.A.CurrentStatus = "Err=" + parametrToString.errCode;
			All.A.ErrCode = parametrToString.errCode;
			All.A.ErrHelp = parametrToString.errStr;
			All.Lg.SaveTextToLog("GetCheckFNbyUID", strXML, StatusBarLog());
			return false;
		}
		if (All.f.IntegerGetFn(parametrToString.ReturnStr, "On") < 1)
		{
			parametrToString.errCode = 1;
			parametrToString.errStr = "Доступ к Фискальному Номеру " + parametrToString.ReturnStr + " запрещен.";
			All.A.CurrentStatus = "Err=" + parametrToString.errCode;
			All.A.ErrHelp = parametrToString.errStr;
			All.A.ErrCode = parametrToString.errCode;
			All.Lg.SaveTextToLog("GetCheckFNbyUID", strXML, StatusBarLog());
			return false;
		}
		if (All.A.Status)
		{
			if (Operators.CompareString(parametrToString.ReturnStr, All.A.FN, false) != 0)
			{
				parametrToString.errCode = 2;
				parametrToString.errStr = "Ранее был подключен другой ФН.";
				All.A.CurrentStatus = "Err=" + parametrToString.errCode;
				All.A.ErrHelp = parametrToString.errStr;
				All.A.ErrCode = parametrToString.errCode;
				All.Lg.SaveTextToLog("GetCheckFNbyUID", strXML, StatusBarLog());
				return false;
			}
			parametrToString = All.d.GetParametrToString(strXML, "uid", "InputParameters/Parameters", RegUpLow: true);
			if (Operators.CompareString(parametrToString.ReturnStr.Trim(), "", false) == 0)
			{
				parametrToString = All.d.GetParametrToString(strXML, "UID", "InputParameters/Parameters", RegUpLow: true);
			}
			if (Operators.CompareString(parametrToString.ReturnStr.Trim(), "", false) == 0)
			{
				parametrToString = All.d.GetParametrToString(strXML, "Uid", "InputParameters/Parameters", RegUpLow: true);
			}
			string returnStr = parametrToString.ReturnStr;
			TypShiftCheckID typShiftCheckID = All.l.UidToTaxId(returnStr);
			All.A.CurrentStatus = "Err=0_FN=" + All.A.FN + "_CheckID=" + typShiftCheckID.CheckID + "_ShiftID=" + typShiftCheckID.ShiftID;
			All.A.ErrHelp = "";
			All.A.ErrCode = 0;
			All.Lg.SaveTextToLog("GetCheckFNbyUID", strXML, StatusBarLog());
			return true;
		}
		parametrToString.errCode = 4;
		parametrToString.errStr = "Нет подключения";
		All.A.CurrentStatus = "Err=" + parametrToString.errCode;
		All.A.ErrHelp = parametrToString.errStr;
		All.A.ErrCode = parametrToString.errCode;
		All.Lg.SaveTextToLog("GetCheckFNbyUID", strXML, StatusBarLog());
		return false;
	}

	public bool GetCheckcloudurl(string strXML)
	{
		TypErrStr typErrStr = default(TypErrStr);
		typErrStr.errCode = 0;
		typErrStr.errStr = "";
		typErrStr.ReturnStr = "";
		typErrStr = All.d.GetParametrToString(strXML, "fn");
		if (typErrStr.errCode > 0)
		{
			All.A.CurrentStatus = "Err=" + typErrStr.errCode;
			All.A.ErrHelp = typErrStr.errStr;
			All.A.ErrCode = typErrStr.errCode;
			All.Lg.SaveTextToLog("GetCheckcloudurl", strXML, StatusBarLog());
			return false;
		}
		if (All.A.Status)
		{
			if (Operators.CompareString(typErrStr.ReturnStr, All.A.FN, false) != 0)
			{
				typErrStr.errCode = 2;
				typErrStr.errStr = "Ранее был подключен другой ФН.";
				All.A.CurrentStatus = "Err=" + typErrStr.errCode;
				All.A.ErrHelp = typErrStr.errStr;
				All.A.ErrCode = typErrStr.errCode;
				All.Lg.SaveTextToLog("GetCheckcloudurl", strXML, StatusBarLog());
				return false;
			}
			typErrStr = All.d.GetParametrToString(strXML, "TaxNum", "InputParameters/Parameters", RegUpLow: true);
			if (typErrStr.errCode > 0)
			{
				All.A.CurrentStatus = "Err=" + typErrStr.errCode;
				All.A.ErrHelp = typErrStr.errStr;
				All.A.ErrCode = typErrStr.errCode;
				All.Lg.SaveTextToLog("GetCheckcloudurl", strXML, StatusBarLog());
				return false;
			}
			TypPrintChecks typPrintChecks = new Reports().CheckXMLNumberTax(typErrStr.ReturnStr);
			if (typPrintChecks.ReturnStr.Trim().Length < 9)
			{
				All.A.ErrHelp = "Чек не найден";
				All.A.ErrCode = 98;
				All.A.CurrentStatus = "Err=" + All.A.ErrCode;
				All.Lg.SaveTextToLog("GetCheckcloudurl", strXML, StatusBarLog());
				return false;
			}
			if ((Operators.CompareString(typPrintChecks.ReturnDocType, "0", false) == 0) | (Operators.CompareString(typPrintChecks.ReturnDocType, "1", false) == 0))
			{
				typPrintChecks.ReturnDocType = typPrintChecks.ReturnDocType;
				string text = All.MyDoc() + "\\WebCheck\\Temp\\" + All.A.FN + "\\" + typPrintChecks.ReturnStrTaxN + ".pdf";
				if (File.Exists(text))
				{
					try
					{
						FileSystem.DeleteFile(text);
					}
					catch (Exception ex)
					{
						ProjectData.SetProjectError(ex);
						Exception ex2 = ex;
						ProjectData.ClearProjectError();
					}
				}
				if (!All.AccessAWS())
				{
					All.A.ErrHelp = "Нет доступа к серверу";
					All.A.ErrCode = 99;
					All.A.CurrentStatus = "Err=" + All.A.ErrCode;
					All.Lg.SaveTextToLog("GetCheckcloudurl", strXML, StatusBarLog());
					return false;
				}
				new PrintExportCheck().ExportCheckToPDF(text, typPrintChecks.ReturnStr, typPrintChecks.ReturnStrTaxN, lite: false);
				SendFilePDF sendFilePDF = new SendFilePDF();
				int num = 0;
				do
				{
					typErrStr = sendFilePDF.SendPDF(text, typPrintChecks.ReturnStrTaxN + ".pdf");
					if (typErrStr.errCode == 0)
					{
						break;
					}
					num = checked(num + 1);
				}
				while (num <= 1);
				if (typErrStr.errCode > 0)
				{
					All.A.CurrentStatus = "Err=" + typErrStr.errCode;
					All.A.ErrHelp = typErrStr.errStr;
					All.A.ErrCode = typErrStr.errCode;
					All.Lg.SaveTextToLog("GetCheckcloudurl", strXML, StatusBarLog());
					return false;
				}
				typPrintChecks.ReturnStrTaxN = typPrintChecks.ReturnStrTaxN.Replace("_", "`");
				All.A.CurrentStatus = "Err=0_FN=" + All.A.FN + "_URL=https://che.ck.ua/" + typPrintChecks.ReturnStrTaxN + ".pdf";
				All.A.ErrHelp = "";
				All.A.ErrCode = 0;
				All.Lg.SaveTextToLog("GetCheckcloudurl", strXML, StatusBarLog());
				return true;
			}
			All.A.ErrHelp = "Ошибка! Тип чека не является публичным.";
			All.A.ErrCode = 99;
			All.A.CurrentStatus = "Err=" + All.A.ErrCode;
			All.Lg.SaveTextToLog("GetCheckcloudurl", strXML, StatusBarLog());
			return false;
		}
		typErrStr.errCode = 4;
		typErrStr.errStr = "Нет подключения";
		All.A.CurrentStatus = "Err=" + typErrStr.errCode;
		All.A.ErrHelp = typErrStr.errStr;
		All.A.ErrCode = typErrStr.errCode;
		return false;
	}

	public string GetStatus()
	{
		string s = "status=ok_version=6.0.7.1331";
		return All.d.GetParametrToXML(s).ReturnStr;
	}

	public bool WriteSettingsINI(string sFN, string sName, string sValue)
	{
		if (!All.A.Status)
		{
			return false;
		}
		if (Operators.CompareString(sFN, All.A.FN, false) != 0)
		{
			return false;
		}
		if ((sFN.Length == 10) & Versioned.IsNumeric((object)sFN))
		{
			All.f.StringWriteFN(sFN, sName, sValue);
			All.Lg.SaveTextToLog("WriteSettingsINI", sFN, sName + "=" + sValue);
		}
		else
		{
			All.f.WriteString("Global", sName, sValue);
			All.Lg.SaveTextToLog("WriteSettingsINI", "Global", sName + "=" + sValue);
		}
		return true;
	}

	public string GetSettingsINI(string sFN, string sName)
	{
		string result = "";
		if (!All.A.Status)
		{
			return result;
		}
		if (Operators.CompareString(sFN, All.A.FN, false) != 0)
		{
			return result;
		}
		return (!((sFN.Length == 10) & Versioned.IsNumeric((object)sFN))) ? All.f.GetString("Global", sName) : All.f.StringGetFn(sFN, sName);
	}

	public bool DeleteDemoBase()
	{
		bool flag;
		bool result;
		try
		{
			flag = new DelFN().DeleteFN();
		}
		catch (Exception ex)
		{
			ProjectData.SetProjectError(ex);
			Exception ex2 = ex;
			result = false;
			ProjectData.ClearProjectError();
			goto IL_0024;
		}
		result = flag;
		goto IL_0024;
		IL_0024:
		return result;
	}

	private bool AddAdvancePayment()
	{
		int num = 0;
		TypErrStr typErrStr = All.l.MaxID("PayForms");
		num = ((!Versioned.IsNumeric((object)typErrStr.ReturnStr)) ? 999 : checked(Conversions.ToInteger(typErrStr.ReturnStr) + 1));
		string fN = All.A.FN;
		fN = ((Operators.CompareString(All.A.FiscalMode, "cabinet.tax.gov.ua:9443", false) != 0) ? All.A.FN : (fN + "_TS"));
		if (new CreateDB(fN).SavePayForms("Сертифікат", num.ToString()).errCode > 0)
		{
			return false;
		}
		return true;
	}

	public string StatusFN()
	{
		if (All.A.Status)
		{
			return All.A.FN;
		}
		return "";
	}

	public bool SetSignSettings(string strXML)
	{
		TypErrStr parametrToString = All.d.GetParametrToString(strXML, "keypath");
		if (parametrToString.errCode == 0)
		{
			All.A.PathKey = parametrToString.ReturnStr;
		}
		else
		{
			All.A.PathKey = "";
		}
		parametrToString = All.d.GetParametrToString(strXML, "keypass", "InputParameters/Parameters", RegUpLow: true);
		if (parametrToString.errCode > 0)
		{
			parametrToString = All.d.GetParametrToString(strXML, "KeyPass", "InputParameters/Parameters", RegUpLow: true);
		}
		if (parametrToString.errCode > 0)
		{
			parametrToString = All.d.GetParametrToString(strXML, "KEYPASS", "InputParameters/Parameters", RegUpLow: true);
		}
		if (parametrToString.errCode > 0)
		{
			parametrToString = All.d.GetParametrToString(strXML, "Keypass", "InputParameters/Parameters", RegUpLow: true);
		}
		if (parametrToString.errCode > 0)
		{
			parametrToString = All.d.GetParametrToString(strXML, "keyPass", "InputParameters/Parameters", RegUpLow: true);
		}
		if (parametrToString.errCode == 0)
		{
			All.A.PassKey = parametrToString.ReturnStr;
			return true;
		}
		All.A.PassKey = "";
		return false;
	}

	public bool Signfileiit(string strXML)
	{
		TypErrStr parametrToString = All.d.GetParametrToString(strXML, "keypath");
		if (parametrToString.errCode == 0)
		{
			All.A.PathKey = parametrToString.ReturnStr;
			if (!File.Exists(All.A.PathKey))
			{
				All.LgAll.SaveTextToLogAll("Signfileiit", "Помилка! Немає файлу ключа.");
				All.A.PathKey = "";
				All.A.PassKey = "";
				return false;
			}
			parametrToString = All.d.GetParametrToString(strXML, "keypass", "InputParameters/Parameters", RegUpLow: true);
			if (parametrToString.errCode > 0)
			{
				parametrToString = All.d.GetParametrToString(strXML, "KeyPass", "InputParameters/Parameters", RegUpLow: true);
			}
			if (parametrToString.errCode > 0)
			{
				parametrToString = All.d.GetParametrToString(strXML, "KEYPASS", "InputParameters/Parameters", RegUpLow: true);
			}
			if (parametrToString.errCode > 0)
			{
				parametrToString = All.d.GetParametrToString(strXML, "Keypass", "InputParameters/Parameters", RegUpLow: true);
			}
			if (parametrToString.errCode > 0)
			{
				parametrToString = All.d.GetParametrToString(strXML, "keyPass", "InputParameters/Parameters", RegUpLow: true);
			}
			if (parametrToString.errCode == 0)
			{
				All.A.PassKey = parametrToString.ReturnStr;
				parametrToString = All.d.GetParametrToString(strXML, "FiletoSign");
				string text = "";
				if (parametrToString.errCode == 0)
				{
					text = parametrToString.ReturnStr;
					if (!File.Exists(text))
					{
						All.LgAll.SaveTextToLogAll("Signfileiit", "Помилка! Немає файлу для підпису.");
						All.A.PathKey = "";
						All.A.PassKey = "";
						return false;
					}
					TypErr typErr = All.SF.SignatureFile(All.A.PathKey, All.A.PathKey, text);
					if (typErr.errCode > 0)
					{
						All.LgAll.SaveTextToLogAll("Signfileiit", typErr.errStr);
						All.A.PathKey = "";
						All.A.PassKey = "";
						return false;
					}
					All.A.PathKey = "";
					All.A.PassKey = "";
					return true;
				}
				All.LgAll.SaveTextToLogAll("Signfileiit", parametrToString.ReturnStr);
				All.A.PathKey = "";
				All.A.PassKey = "";
				return false;
			}
			All.LgAll.SaveTextToLogAll("Signfileiit", parametrToString.ReturnStr);
			All.A.PathKey = "";
			All.A.PassKey = "";
			return false;
		}
		All.LgAll.SaveTextToLogAll("Signfileiit", parametrToString.ReturnStr);
		All.A.PathKey = "";
		All.A.PassKey = "";
		return false;
	}

	private void OfflineOnZ()
	{
		if (Operators.CompareString(All.f.GetString("Global", "OfflineReportZ"), "1", false) == 0)
		{
			string strXML = "<InputParameters><Parameters FN ='" + All.A.FN + "'/></InputParameters>";
			OnlineToOffline(strXML);
		}
	}

	private void OfflineOnC()
	{
		if (Operators.CompareString(All.f.GetString("Global", "OfflineCheck"), "1", false) == 0)
		{
			string strXML = "<InputParameters><Parameters FN ='" + All.A.FN + "'/></InputParameters>";
			OnlineToOffline(strXML);
		}
	}

	private bool CreateTables(string FnS)
	{
		CreateDB createDB = new CreateDB(FnS);
		int num = 0;
		do
		{
			createDB.CreateTable(num);
			num = checked(num + 1);
		}
		while (num <= 13);
		createDB.CreateTriger();
		createDB.CreateTriger1();
		createDB.CreateTriger2();
		createDB.CreateTrigerBackup();
		createDB.CreateIndex(newPRO: true);
		return true;
	}

	public bool CreateRow(string FnS, string TinT, string InnT, string NtorgT, string NorgT, string AtorgT, string NewPar = "")
	{
		CreateDB createDB = new CreateDB(FnS);
		createDB.SaveTaxObjects(FnS, TinT, InnT, All.l.TextToTextSQL(NtorgT), All.l.TextToTextSQL(NorgT), All.l.TextToTextSQL(AtorgT));
		int num = 1;
		do
		{
			createDB.SaveInfoTable(num);
			num = checked(num + 1);
		}
		while (num <= 14);
		return true;
	}

	public bool SendMessage(string strXML)
	{
		InViber inViber = new InViber();
		TypErr typErr = default(TypErr);
		typErr.errCode = 0;
		typErr.errStr = "";
		TypErrStr parametrToString = All.d.GetParametrToString(strXML, "messagetype");
		parametrToString.ReturnStr = parametrToString.ReturnStr.Trim();
		int num = ((!Versioned.IsNumeric((object)parametrToString.ReturnStr)) ? 3 : Conversions.ToInteger(parametrToString.ReturnStr));
		if (num == 4)
		{
			typErr = inViber.InTextViber();
			if (typErr.errCode > 0)
			{
				All.A.CurrentStatus = "Err=" + typErr.errCode;
				All.A.ErrHelp = typErr.errStr;
				All.A.ErrCode = typErr.errCode;
				All.Lg.SaveTextToLog("SendMessage", strXML, StatusBarLog());
				return false;
			}
			All.A.CurrentStatus = "Err=0_FN=" + All.A.FN + "_Remainder =" + typErr.errStr;
			All.A.ErrHelp = "";
			All.A.ErrCode = 0;
			All.Lg.SaveTextToLog("SendMessage 4", strXML, StatusBarLog());
			return true;
		}
		parametrToString = All.d.GetParametrToString(strXML, "CheckID", "InputParameters/Parameters", RegUpLow: true);
		if (parametrToString.errCode > 0)
		{
			All.A.CurrentStatus = "Err=" + parametrToString.errCode;
			All.A.ErrHelp = parametrToString.errStr;
			All.A.ErrCode = parametrToString.errCode;
			All.Lg.SaveTextToLog("SendMessage", strXML, StatusBarLog());
			return false;
		}
		string returnStr = parametrToString.ReturnStr;
		parametrToString = All.d.GetParametrToString(strXML, "Dest");
		if (parametrToString.errCode > 0)
		{
			All.A.CurrentStatus = "Err=" + parametrToString.errCode;
			All.A.ErrHelp = parametrToString.errStr;
			All.A.ErrCode = parametrToString.errCode;
			All.Lg.SaveTextToLog("SendMessage", strXML, StatusBarLog());
			return false;
		}
		string returnStr2 = parametrToString.ReturnStr;
		typErr = inViber.InTextViber(returnStr, returnStr2, num);
		if (typErr.errCode > 0)
		{
			All.A.CurrentStatus = "Err=" + typErr.errCode;
			All.A.ErrHelp = typErr.errStr;
			All.A.ErrCode = typErr.errCode;
			All.Lg.SaveTextToLog("SendMessage", strXML, StatusBarLog());
			return false;
		}
		All.A.CurrentStatus = "Err=0_FN=" + All.A.FN + "_Remainder =" + typErr.errStr;
		All.A.ErrHelp = "";
		All.A.ErrCode = 0;
		All.Lg.SaveTextToLog("SendMessage", strXML, StatusBarLog());
		return true;
	}

	public bool GetShiftStatus(string strXML)
	{
		TypErrStr parametrToString = All.d.GetParametrToString(strXML, "fn");
		if (parametrToString.errCode > 0)
		{
			All.A.CurrentStatus = "Err=" + parametrToString.errCode;
			All.A.ErrHelp = parametrToString.errStr;
			All.A.ErrCode = parametrToString.errCode;
			All.Lg.SaveTextToLog("GetShiftStatus", strXML, StatusBarLog());
			return false;
		}
		if (!Versioned.IsNumeric((object)parametrToString.ReturnStr) | (parametrToString.ReturnStr.Length != 10))
		{
			parametrToString.errCode = 3;
			parametrToString.errStr = "Неправильний формат Фіскального Номера: " + parametrToString.ReturnStr;
			All.A.CurrentStatus = "Err=" + parametrToString.errCode;
			All.A.ErrHelp = parametrToString.errStr;
			All.A.ErrCode = parametrToString.errCode;
			All.Lg.SaveTextToLog("GetShiftStatus", strXML, StatusBarLog());
			return false;
		}
		if (Operators.CompareString(All.f.StringGetFn(parametrToString.ReturnStr, "Path"), "", false) == 0)
		{
			parametrToString.errCode = 1001;
			parametrToString.errStr = "Фіскального номера " + parametrToString.ReturnStr + " у базі немає.";
			All.A.CurrentStatus = "Err=" + parametrToString.errCode;
			All.A.ErrCode = parametrToString.errCode;
			All.A.ErrHelp = parametrToString.errStr;
			All.Lg.SaveTextToLog("GetShiftStatus", strXML, StatusBarLog());
			return false;
		}
		if (All.f.IntegerGetFn(parametrToString.ReturnStr, "On") < 1)
		{
			parametrToString.errCode = 1;
			parametrToString.errStr = "Доступ к Фискальному Номеру " + parametrToString.ReturnStr + " запрещен.";
			All.A.CurrentStatus = "Err=" + parametrToString.errCode;
			All.A.ErrHelp = parametrToString.errStr;
			All.A.ErrCode = parametrToString.errCode;
			All.Lg.SaveTextToLog("GetShiftStatus", strXML, StatusBarLog());
			return false;
		}
		if (All.A.Status)
		{
			if (Operators.CompareString(parametrToString.ReturnStr, All.A.FN, false) != 0)
			{
				parametrToString.errCode = 2;
				parametrToString.errStr = "Ранее был подключен другой ФН.";
				All.A.CurrentStatus = "Err=" + parametrToString.errCode;
				All.A.ErrHelp = parametrToString.errStr;
				All.A.ErrCode = parametrToString.errCode;
				All.Lg.SaveTextToLog("GetShiftStatus", strXML, StatusBarLog());
				return false;
			}
			string returnStr = All.l.ReturnOpenShift().ReturnStr;
			All.A.CurrentStatus = "Err=0_FN=" + All.A.FN + "_ShiftNumber=" + returnStr;
			All.A.ErrHelp = "";
			All.A.ErrCode = 0;
			All.Lg.SaveTextToLog("GetShiftStatus", strXML, StatusBarLog());
			return true;
		}
		parametrToString.errCode = 4;
		parametrToString.errStr = "Нет подключения";
		All.A.CurrentStatus = "Err=" + parametrToString.errCode;
		All.A.ErrHelp = parametrToString.errStr;
		All.A.ErrCode = parametrToString.errCode;
		All.Lg.SaveTextToLog("GetShiftStatus", strXML, StatusBarLog());
		return false;
	}

	public bool InitializationFormal(string FNstr, string PathBase, string eTin)
	{
		if (All.A.TypWork != 2025)
		{
			return false;
		}
		All.A.FN = FNstr.Trim();
		if (!Versioned.IsNumeric((object)All.A.FN) | (All.A.FN.Length != 10))
		{
			return false;
		}
		if (All.A.Status)
		{
			return false;
		}
		All.A.FileN = PathBase.Trim();
		if (Operators.CompareString(All.A.FileN, "", false) == 0)
		{
			return false;
		}
		All.A.TIN = eTin;
		All.A.Connection = "Data Source=" + All.A.FileN + "; Version=3";
		All.A.Status = true;
		All.SettingsForServer();
		All.IsFullVersion();
		if (Operators.CompareString(All.A.FN, "7000000512", false) == 0)
		{
			All.A.FullVersion = true;
		}
		return true;
	}

	public bool OpenShiftTime(string eMinutes)
	{
		if (!All.A.Status)
		{
			return false;
		}
		return All.l.OST(eMinutes);
	}
}
