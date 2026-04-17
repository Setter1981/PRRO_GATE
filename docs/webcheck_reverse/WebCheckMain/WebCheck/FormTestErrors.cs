using System;
using System.ComponentModel;
using System.Diagnostics;
using System.Drawing;
using System.IO;
using System.Runtime.CompilerServices;
using System.Text;
using System.Windows.Forms;
using Microsoft.VisualBasic.CompilerServices;
using Microsoft.VisualBasic.Devices;
using Microsoft.VisualBasic.FileIO;
using WebCheck.My;

namespace WebCheck;

[DesignerGenerated]
public class FormTestErrors : Form
{
	private IContainer components;

	[CompilerGenerated]
	[AccessedThroughProperty("NoB")]
	private Button _NoB;

	[CompilerGenerated]
	[AccessedThroughProperty("OkB")]
	private Button _OkB;

	[CompilerGenerated]
	[AccessedThroughProperty("CopyResult")]
	private Button _CopyResult;

	[CompilerGenerated]
	[AccessedThroughProperty("РозпочатиТестуванняToolStripMenuItem")]
	private ToolStripMenuItem _РозпочатиТестуванняToolStripMenuItem;

	[CompilerGenerated]
	[AccessedThroughProperty("ОновитиСертифікатиToolStripMenuItem")]
	private ToolStripMenuItem _ОновитиСертифікатиToolStripMenuItem;

	private bool CloseTest;

	private bool StopTest;

	internal virtual Button NoB
	{
		[CompilerGenerated]
		get
		{
			return _NoB;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler eventHandler = NoB_Click;
			Button noB = _NoB;
			if (noB != null)
			{
				((Control)noB).Click -= eventHandler;
			}
			_NoB = value;
			noB = _NoB;
			if (noB != null)
			{
				((Control)noB).Click += eventHandler;
			}
		}
	}

	internal virtual Button OkB
	{
		[CompilerGenerated]
		get
		{
			return _OkB;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler eventHandler = OkB_Click;
			Button okB = _OkB;
			if (okB != null)
			{
				((Control)okB).Click -= eventHandler;
			}
			_OkB = value;
			okB = _OkB;
			if (okB != null)
			{
				((Control)okB).Click += eventHandler;
			}
		}
	}

	[field: AccessedThroughProperty("TestResult")]
	internal virtual TextBox TestResult
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	internal virtual Button CopyResult
	{
		[CompilerGenerated]
		get
		{
			return _CopyResult;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler eventHandler = CopyResult_Click;
			Button copyResult = _CopyResult;
			if (copyResult != null)
			{
				((Control)copyResult).Click -= eventHandler;
			}
			_CopyResult = value;
			copyResult = _CopyResult;
			if (copyResult != null)
			{
				((Control)copyResult).Click += eventHandler;
			}
		}
	}

	[field: AccessedThroughProperty("MenuStrip1")]
	internal virtual MenuStrip MenuStrip1
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	[field: AccessedThroughProperty("МенюToolStripMenuItem")]
	internal virtual ToolStripMenuItem МенюToolStripMenuItem
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	internal virtual ToolStripMenuItem РозпочатиТестуванняToolStripMenuItem
	{
		[CompilerGenerated]
		get
		{
			return _РозпочатиТестуванняToolStripMenuItem;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler eventHandler = РозпочатиТестуванняToolStripMenuItem_Click;
			ToolStripMenuItem розпочатиТестуванняToolStripMenuItem = _РозпочатиТестуванняToolStripMenuItem;
			if (розпочатиТестуванняToolStripMenuItem != null)
			{
				((ToolStripItem)розпочатиТестуванняToolStripMenuItem).Click -= eventHandler;
			}
			_РозпочатиТестуванняToolStripMenuItem = value;
			розпочатиТестуванняToolStripMenuItem = _РозпочатиТестуванняToolStripMenuItem;
			if (розпочатиТестуванняToolStripMenuItem != null)
			{
				((ToolStripItem)розпочатиТестуванняToolStripMenuItem).Click += eventHandler;
			}
		}
	}

	[field: AccessedThroughProperty("ToolStripMenuItem1")]
	internal virtual ToolStripSeparator ToolStripMenuItem1
	{
		get; [MethodImpl(MethodImplOptions.Synchronized)]
		set;
	}

	internal virtual ToolStripMenuItem ОновитиСертифікатиToolStripMenuItem
	{
		[CompilerGenerated]
		get
		{
			return _ОновитиСертифікатиToolStripMenuItem;
		}
		[MethodImpl(MethodImplOptions.Synchronized)]
		[CompilerGenerated]
		set
		{
			EventHandler eventHandler = ОновитиСертифікатиToolStripMenuItem_Click;
			ToolStripMenuItem оновитиСертифікатиToolStripMenuItem = _ОновитиСертифікатиToolStripMenuItem;
			if (оновитиСертифікатиToolStripMenuItem != null)
			{
				((ToolStripItem)оновитиСертифікатиToolStripMenuItem).Click -= eventHandler;
			}
			_ОновитиСертифікатиToolStripMenuItem = value;
			оновитиСертифікатиToolStripMenuItem = _ОновитиСертифікатиToolStripMenuItem;
			if (оновитиСертифікатиToolStripMenuItem != null)
			{
				((ToolStripItem)оновитиСертифікатиToolStripMenuItem).Click += eventHandler;
			}
		}
	}

	public FormTestErrors()
	{
		((Form)this).Load += FormTestErrors_Load;
		((Form)this).Closing += FormTestErrors_Closing;
		CloseTest = true;
		StopTest = false;
		InitializeComponent();
	}

	[DebuggerNonUserCode]
	protected override void Dispose(bool disposing)
	{
		try
		{
			if (disposing && components != null)
			{
				components.Dispose();
			}
		}
		finally
		{
			((Form)this).Dispose(disposing);
		}
	}

	[DebuggerStepThrough]
	private void InitializeComponent()
	{
		//IL_0011: Unknown result type (might be due to invalid IL or missing references)
		//IL_001b: Expected O, but got Unknown
		//IL_001c: Unknown result type (might be due to invalid IL or missing references)
		//IL_0026: Expected O, but got Unknown
		//IL_0027: Unknown result type (might be due to invalid IL or missing references)
		//IL_0031: Expected O, but got Unknown
		//IL_0032: Unknown result type (might be due to invalid IL or missing references)
		//IL_003c: Expected O, but got Unknown
		//IL_003d: Unknown result type (might be due to invalid IL or missing references)
		//IL_0047: Expected O, but got Unknown
		//IL_0048: Unknown result type (might be due to invalid IL or missing references)
		//IL_0052: Expected O, but got Unknown
		//IL_0053: Unknown result type (might be due to invalid IL or missing references)
		//IL_005d: Expected O, but got Unknown
		//IL_005e: Unknown result type (might be due to invalid IL or missing references)
		//IL_0068: Expected O, but got Unknown
		//IL_0069: Unknown result type (might be due to invalid IL or missing references)
		//IL_0073: Expected O, but got Unknown
		//IL_009b: Unknown result type (might be due to invalid IL or missing references)
		//IL_00a5: Expected O, but got Unknown
		//IL_0126: Unknown result type (might be due to invalid IL or missing references)
		//IL_0130: Expected O, but got Unknown
		//IL_01b1: Unknown result type (might be due to invalid IL or missing references)
		//IL_01bb: Expected O, but got Unknown
		//IL_024d: Unknown result type (might be due to invalid IL or missing references)
		//IL_0257: Expected O, but got Unknown
		//IL_04d9: Unknown result type (might be due to invalid IL or missing references)
		//IL_04e3: Expected O, but got Unknown
		ComponentResourceManager componentResourceManager = new ComponentResourceManager(typeof(FormTestErrors));
		NoB = new Button();
		OkB = new Button();
		TestResult = new TextBox();
		CopyResult = new Button();
		MenuStrip1 = new MenuStrip();
		МенюToolStripMenuItem = new ToolStripMenuItem();
		ОновитиСертифікатиToolStripMenuItem = new ToolStripMenuItem();
		РозпочатиТестуванняToolStripMenuItem = new ToolStripMenuItem();
		ToolStripMenuItem1 = new ToolStripSeparator();
		((Control)MenuStrip1).SuspendLayout();
		((Control)this).SuspendLayout();
		((Control)NoB).Font = new Font("Microsoft Sans Serif", 12f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)NoB).Location = new Point(258, 568);
		((Control)NoB).Name = "NoB";
		((Control)NoB).Size = new Size(283, 37);
		((Control)NoB).TabIndex = 14;
		((ButtonBase)NoB).Text = "Скасувати тестування";
		((ButtonBase)NoB).UseVisualStyleBackColor = true;
		((Control)OkB).Font = new Font("Microsoft Sans Serif", 12f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)OkB).Location = new Point(556, 568);
		((Control)OkB).Name = "OkB";
		((Control)OkB).Size = new Size(283, 37);
		((Control)OkB).TabIndex = 13;
		((ButtonBase)OkB).Text = "Розпочати тестування";
		((ButtonBase)OkB).UseVisualStyleBackColor = true;
		((Control)TestResult).Font = new Font("Microsoft Sans Serif", 12f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)TestResult).Location = new Point(12, 46);
		TestResult.Multiline = true;
		((Control)TestResult).Name = "TestResult";
		((TextBoxBase)TestResult).ReadOnly = true;
		TestResult.ScrollBars = (ScrollBars)2;
		((Control)TestResult).Size = new Size(827, 511);
		((Control)TestResult).TabIndex = 12;
		((Control)TestResult).TabStop = false;
		((Control)CopyResult).Font = new Font("Microsoft Sans Serif", 12f, (FontStyle)0, (GraphicsUnit)3, (byte)204);
		((Control)CopyResult).Location = new Point(12, 568);
		((Control)CopyResult).Name = "CopyResult";
		((Control)CopyResult).Size = new Size(222, 37);
		((Control)CopyResult).TabIndex = 15;
		((ButtonBase)CopyResult).Text = "У буфер обміну";
		((ButtonBase)CopyResult).UseVisualStyleBackColor = true;
		((ToolStrip)MenuStrip1).ImageScalingSize = new Size(20, 20);
		((ToolStrip)MenuStrip1).Items.AddRange((ToolStripItem[])(object)new ToolStripItem[1] { (ToolStripItem)МенюToolStripMenuItem });
		((Control)MenuStrip1).Location = new Point(0, 0);
		((Control)MenuStrip1).Name = "MenuStrip1";
		((Control)MenuStrip1).Size = new Size(853, 28);
		((Control)MenuStrip1).TabIndex = 16;
		((Control)MenuStrip1).Text = "MenuStrip1";
		((ToolStripDropDownItem)МенюToolStripMenuItem).DropDownItems.AddRange((ToolStripItem[])(object)new ToolStripItem[3]
		{
			(ToolStripItem)РозпочатиТестуванняToolStripMenuItem,
			(ToolStripItem)ToolStripMenuItem1,
			(ToolStripItem)ОновитиСертифікатиToolStripMenuItem
		});
		((ToolStripItem)МенюToolStripMenuItem).Name = "МенюToolStripMenuItem";
		((ToolStripItem)МенюToolStripMenuItem).Size = new Size(65, 24);
		((ToolStripItem)МенюToolStripMenuItem).Text = "Меню";
		((ToolStripItem)ОновитиСертифікатиToolStripMenuItem).Name = "ОновитиСертифікатиToolStripMenuItem";
		((ToolStripItem)ОновитиСертифікатиToolStripMenuItem).Size = new Size(245, 26);
		((ToolStripItem)ОновитиСертифікатиToolStripMenuItem).Text = "Оновити сертифікати";
		((ToolStripItem)РозпочатиТестуванняToolStripMenuItem).Name = "РозпочатиТестуванняToolStripMenuItem";
		((ToolStripItem)РозпочатиТестуванняToolStripMenuItem).Size = new Size(245, 26);
		((ToolStripItem)РозпочатиТестуванняToolStripMenuItem).Text = "Розпочати тестування";
		((ToolStripItem)ToolStripMenuItem1).Name = "ToolStripMenuItem1";
		((ToolStripItem)ToolStripMenuItem1).Size = new Size(242, 6);
		((ContainerControl)this).AutoScaleDimensions = new SizeF(8f, 16f);
		((ContainerControl)this).AutoScaleMode = (AutoScaleMode)1;
		((Form)this).ClientSize = new Size(853, 613);
		((Control)this).Controls.Add((Control)(object)CopyResult);
		((Control)this).Controls.Add((Control)(object)NoB);
		((Control)this).Controls.Add((Control)(object)OkB);
		((Control)this).Controls.Add((Control)(object)TestResult);
		((Control)this).Controls.Add((Control)(object)MenuStrip1);
		((Form)this).FormBorderStyle = (FormBorderStyle)1;
		((Form)this).Icon = (Icon)componentResourceManager.GetObject("$this.Icon");
		((Form)this).MainMenuStrip = MenuStrip1;
		((Form)this).MaximizeBox = false;
		((Form)this).MinimizeBox = false;
		((Control)this).Name = "FormTestErrors";
		((Form)this).StartPosition = (FormStartPosition)1;
		((Form)this).Text = "Пошук та усунення несправностей";
		((Control)MenuStrip1).ResumeLayout(false);
		((Control)MenuStrip1).PerformLayout();
		((Control)this).ResumeLayout(false);
		((Control)this).PerformLayout();
	}

	private void FormTestErrors_Load(object sender, EventArgs e)
	{
		((Control)OkB).Enabled = true;
		((Control)NoB).Enabled = false;
		TestResult.Text = "- Підключено фіскальний номер " + All.A.FN;
		((Form)this).Text = ((Form)this).Text + "           ( WebCheck0.dll  v.6.0.7.1331 )";
	}

	private void FormTestErrors_Closing(object sender, CancelEventArgs e)
	{
		if (!((Control)OkB).Enabled)
		{
			StopTest = true;
			((Control)NoB).Enabled = false;
			CloseTest = true;
			e.Cancel = true;
		}
		else
		{
			e.Cancel = false;
		}
	}

	private void CopyResult_Click(object sender, EventArgs e)
	{
		Clipboard.SetText(TestResult.Text);
	}

	private void OkB_Click(object sender, EventArgs e)
	{
		TestStart();
	}

	private void TestStart()
	{
		CloseTest = false;
		МенюToolStripMenuItem.Enabled = false;
		StopTest = false;
		((Control)OkB).Enabled = false;
		((Control)NoB).Enabled = true;
		StartTestErrors();
		((Control)OkB).Enabled = true;
		((Control)NoB).Enabled = false;
		МенюToolStripMenuItem.Enabled = true;
		CloseTest = true;
	}

	private bool SaveLastCheck()
	{
		bool result;
		try
		{
			TypPrintChecks typPrintChecks = new Reports().CheckXMLlast();
			string text = All.MyDoc() + "\\WebCheck\\Temp\\" + All.A.FN + "\\000.xml";
			if (File.Exists(text))
			{
				FileSystem.DeleteFile(text);
				Application.DoEvents();
			}
			All.SaveToFileText(text, typPrintChecks.ReturnStr);
			Application.DoEvents();
		}
		catch (Exception ex)
		{
			ProjectData.SetProjectError(ex);
			Exception ex2 = ex;
			result = false;
			ProjectData.ClearProjectError();
			goto IL_0063;
		}
		result = true;
		goto IL_0063;
		IL_0063:
		return result;
	}

	private void NoB_Click(object sender, EventArgs e)
	{
		StopTest = true;
		((Control)NoB).Enabled = false;
	}

	private void StartTestErrors()
	{
		StringBuilder stringBuilder = new StringBuilder();
		TestResult.Text = "";
		Application.DoEvents();
		stringBuilder.Append("- Підключено фіскальний номер " + All.A.FN + " - Початок тесту!");
		stringBuilder.Append("\r\n");
		stringBuilder.Append("- Адреса торгової точки: " + All.A.PointAddr);
		stringBuilder.Append("\r\n");
		stringBuilder.Append("- Число рядків  Ksef: " + All.l.CountMax("ksef").ReturnStr + "   CheckHead: " + All.l.CountMax("CHECKHEAD").ReturnStr + "   Shifts: " + All.l.CountMax("SHIFTS").ReturnStr);
		if (All.A.FullVersion)
		{
			if (All.l.OfflineTrue())
			{
				stringBuilder.Append("\r\n");
				stringBuilder.Append("- Статус: офлайн");
				string text = All.l.OfflineDate().ReturnStr.Trim();
				stringBuilder.Append("\r\n");
				stringBuilder.Append("- Дата старту: " + text);
				stringBuilder.Append("\r\n");
				stringBuilder.Append("- Залишок офлайн чеків: " + All.l.OfflineCheckCount());
				NumbersOfflineUse numbersOfflineUse = new NumbersOfflineUse();
				stringBuilder.Append("\r\n");
				stringBuilder.Append("- Кількість резервних офлайн номерів: " + numbersOfflineUse.CountNubmers());
				string text2 = All.f.StringGetFn(All.A.FN, "LastOfflineErr");
				if (Operators.CompareString(text2.Trim(), "", false) == 0)
				{
					text2 = "відсутня";
				}
				stringBuilder.Append("\r\n");
				stringBuilder.Append("- Остання помилка: " + text2);
			}
			else
			{
				stringBuilder.Append("\r\n");
				stringBuilder.Append("- Статус: онлайн");
			}
		}
		else
		{
			stringBuilder.Append("\r\n");
			stringBuilder.Append("- Безкоштовна версія!");
		}
		stringBuilder.Append("\r\n");
		string text3;
		try
		{
			text3 = "версія: " + FileVersionInfo.GetVersionInfo("C:\\Program Files (x86)\\WebCheck\\PRRO32\\WebCheck.dll").FileVersion + "xxx";
		}
		catch (Exception ex)
		{
			ProjectData.SetProjectError(ex);
			Exception ex2 = ex;
			text3 = "не встановлено";
			ProjectData.ClearProjectError();
		}
		string text4;
		try
		{
			text4 = "версія: " + FileVersionInfo.GetVersionInfo("C:\\Program Files (x86)\\WebCheck\\PRRO64\\WebCheck.dll").FileVersion + "xxx";
		}
		catch (Exception ex3)
		{
			ProjectData.SetProjectError(ex3);
			Exception ex4 = ex3;
			text4 = "не встановлено";
			ProjectData.ClearProjectError();
		}
		stringBuilder.Append("- PRRO32\\WebCheck.dll " + text3 + "       PRRO64\\WebCheck.dll " + text4);
		string @string = All.f.GetString("Global", "grpcproxy");
		if (Operators.CompareString(@string, "", false) != 0)
		{
			stringBuilder.Append("\r\n");
			stringBuilder.Append("- grpc прокси: " + @string);
		}
		TestResult.Text = stringBuilder.ToString();
		Application.DoEvents();
		if (Operators.CompareString(All.l.CountMax("ksef").ReturnStr, "0", false) == 0)
		{
			((Control)OkB).Enabled = true;
			((Control)NoB).Enabled = false;
			StopTest = false;
			stringBuilder.Append("\r\n");
			stringBuilder.Append("- Увага! Таблиця KSEF не містить записів. Перевірка перервана!");
			TestResult.Text = stringBuilder.ToString();
			Application.DoEvents();
			if (CloseTest)
			{
				((Form)this).Close();
			}
			return;
		}
		if (StopTest)
		{
			((Control)OkB).Enabled = true;
			((Control)NoB).Enabled = false;
			StopTest = false;
			stringBuilder.Append("\r\n");
			stringBuilder.Append("- Перевірка перервана оператором!");
			TestResult.Text = stringBuilder.ToString();
			Application.DoEvents();
			if (CloseTest)
			{
				((Form)this).Close();
			}
			return;
		}
		stringBuilder.Append("\r\n");
		stringBuilder.Append("- Тест 1: Перевірка змін...");
		TestResult.Text = stringBuilder.ToString();
		Application.DoEvents();
		if (Operators.CompareString(All.l.ReturnOpenShift().ReturnStr, "-1", false) == 0)
		{
			stringBuilder.Append("\r\n");
			stringBuilder.Append("- ERROR!");
			TestResult.Text = stringBuilder.ToString();
			Application.DoEvents();
			All.l.BugFix(1);
			All.l.BugFix(2);
			stringBuilder.Append("\r\n");
			stringBuilder.Append("- Bug Fix OK!");
			TestResult.Text = stringBuilder.ToString();
			Application.DoEvents();
		}
		else
		{
			stringBuilder.Append("\r\n");
			stringBuilder.Append("- OK!");
			TestResult.Text = stringBuilder.ToString();
			Application.DoEvents();
		}
		if (StopTest)
		{
			((Control)OkB).Enabled = true;
			((Control)NoB).Enabled = false;
			StopTest = false;
			stringBuilder.Append("\r\n");
			stringBuilder.Append("- Перевірка перервана оператором!");
			TestResult.Text = stringBuilder.ToString();
			Application.DoEvents();
			if (CloseTest)
			{
				((Form)this).Close();
			}
			return;
		}
		stringBuilder.Append("\r\n");
		stringBuilder.Append("- Тест 2: Перевірка помилки зміни в онлайн...");
		TestResult.Text = stringBuilder.ToString();
		Application.DoEvents();
		bool flag = false;
		if (All.l.OfflineTrue() && Operators.CompareString(LastErrorN(), "-15", false) == 0 && All.l.TestBug(1).ReturnStr.Length > 0)
		{
			flag = true;
			stringBuilder.Append("\r\n");
			stringBuilder.Append("- ERROR!");
			TestResult.Text = stringBuilder.ToString();
			Application.DoEvents();
		}
		if (flag)
		{
			All.l.BugFix(3);
			stringBuilder.Append("\r\n");
			stringBuilder.Append("- Bug Fix OK!");
			TestResult.Text = stringBuilder.ToString();
			Application.DoEvents();
		}
		else
		{
			stringBuilder.Append("\r\n");
			stringBuilder.Append("- OK!");
			TestResult.Text = stringBuilder.ToString();
			Application.DoEvents();
		}
		if (StopTest)
		{
			((Control)OkB).Enabled = true;
			((Control)NoB).Enabled = false;
			StopTest = false;
			stringBuilder.Append("\r\n");
			stringBuilder.Append("- Перевірка перервана оператором!");
			TestResult.Text = stringBuilder.ToString();
			Application.DoEvents();
			if (CloseTest)
			{
				((Form)this).Close();
			}
			return;
		}
		stringBuilder.Append("\r\n");
		stringBuilder.Append("- Тест 3: Перевірка коректного закриття зміни...");
		TestResult.Text = stringBuilder.ToString();
		Application.DoEvents();
		if (All.l.TestBug(2).ReturnStr.Length > 0)
		{
			stringBuilder.Append("\r\n");
			stringBuilder.Append("- ERROR!");
			TestResult.Text = stringBuilder.ToString();
			Application.DoEvents();
			All.l.BugFix(4);
			stringBuilder.Append("\r\n");
			stringBuilder.Append("- Bug Fix OK!");
			TestResult.Text = stringBuilder.ToString();
			Application.DoEvents();
		}
		else
		{
			stringBuilder.Append("\r\n");
			stringBuilder.Append("- OK!");
			TestResult.Text = stringBuilder.ToString();
			Application.DoEvents();
		}
		if (StopTest)
		{
			((Control)OkB).Enabled = true;
			((Control)NoB).Enabled = false;
			StopTest = false;
			stringBuilder.Append("\r\n");
			stringBuilder.Append("- Перевірка перервана оператором!");
			TestResult.Text = stringBuilder.ToString();
			Application.DoEvents();
			if (CloseTest)
			{
				((Form)this).Close();
			}
			return;
		}
		stringBuilder.Append("\r\n");
		stringBuilder.Append("- Тест 4: Перевірка змін №2...");
		TestResult.Text = stringBuilder.ToString();
		Application.DoEvents();
		if (Operators.CompareString(All.l.TestBug(3).ReturnStr, "0", false) != 0)
		{
			stringBuilder.Append("\r\n");
			stringBuilder.Append("- ERROR!");
			TestResult.Text = stringBuilder.ToString();
			Application.DoEvents();
			All.l.BugFix(5);
			stringBuilder.Append("\r\n");
			stringBuilder.Append("- Bug Fix OK!");
			TestResult.Text = stringBuilder.ToString();
			Application.DoEvents();
		}
		else
		{
			stringBuilder.Append("\r\n");
			stringBuilder.Append("- OK!");
			TestResult.Text = stringBuilder.ToString();
			Application.DoEvents();
		}
		if (StopTest)
		{
			((Control)OkB).Enabled = true;
			((Control)NoB).Enabled = false;
			StopTest = false;
			stringBuilder.Append("\r\n");
			stringBuilder.Append("- Перевірка перервана оператором!");
			TestResult.Text = stringBuilder.ToString();
			Application.DoEvents();
			if (CloseTest)
			{
				((Form)this).Close();
			}
			return;
		}
		stringBuilder.Append("\r\n");
		stringBuilder.Append("- Тест 5: Перевірка закриття змін №2...");
		TestResult.Text = stringBuilder.ToString();
		Application.DoEvents();
		if (Operators.CompareString(All.l.TestBug(4).ReturnStr, "0", false) != 0)
		{
			stringBuilder.Append("\r\n");
			stringBuilder.Append("- ERROR!");
			TestResult.Text = stringBuilder.ToString();
			Application.DoEvents();
			All.l.BugFix(6);
			stringBuilder.Append("\r\n");
			stringBuilder.Append("- Bug Fix OK!");
			TestResult.Text = stringBuilder.ToString();
			Application.DoEvents();
		}
		else
		{
			stringBuilder.Append("\r\n");
			stringBuilder.Append("- OK!");
			TestResult.Text = stringBuilder.ToString();
			Application.DoEvents();
		}
		if (StopTest)
		{
			((Control)OkB).Enabled = true;
			((Control)NoB).Enabled = false;
			StopTest = false;
			stringBuilder.Append("\r\n");
			stringBuilder.Append("- Перевірка перервана оператором!");
			TestResult.Text = stringBuilder.ToString();
			Application.DoEvents();
			if (CloseTest)
			{
				((Form)this).Close();
			}
			return;
		}
		stringBuilder.Append("\r\n");
		stringBuilder.Append("- Тест 6: Перевірка помилкової позначки офлайн чека");
		TestResult.Text = stringBuilder.ToString();
		Application.DoEvents();
		stringBuilder.Append("\r\n");
		stringBuilder.Append("- OK!");
		TestResult.Text = stringBuilder.ToString();
		Application.DoEvents();
		if (StopTest)
		{
			((Control)OkB).Enabled = true;
			((Control)NoB).Enabled = false;
			StopTest = false;
			stringBuilder.Append("\r\n");
			stringBuilder.Append("- Перевірка перервана оператором!");
			TestResult.Text = stringBuilder.ToString();
			Application.DoEvents();
			if (CloseTest)
			{
				((Form)this).Close();
			}
			return;
		}
		stringBuilder.Append("\r\n");
		stringBuilder.Append("- Тест 7: Очищення та стиснення бази даних");
		TestResult.Text = stringBuilder.ToString();
		Application.DoEvents();
		if (File.Exists(All.MyDoc() + "\\WebCheck\\Backup\\" + All.A.FN + ".db"))
		{
			All.l.BugFix(8);
			All.l.BugFix(9);
			All.l.BugFix(10);
			All.l.BugFix(11);
			stringBuilder.Append("\r\n");
			stringBuilder.Append("- Success!");
			TestResult.Text = stringBuilder.ToString();
			Application.DoEvents();
		}
		else
		{
			stringBuilder.Append("\r\n");
			stringBuilder.Append("- Увага! Очищення та стиснення бази можливе лише при включеному бекапі!");
			TestResult.Text = stringBuilder.ToString();
			Application.DoEvents();
		}
		if (StopTest)
		{
			((Control)OkB).Enabled = true;
			((Control)NoB).Enabled = false;
			StopTest = false;
			stringBuilder.Append("\r\n");
			stringBuilder.Append("- Перевірка перервана оператором!");
			TestResult.Text = stringBuilder.ToString();
			Application.DoEvents();
			if (CloseTest)
			{
				((Form)this).Close();
			}
			return;
		}
		stringBuilder.Append("\r\n");
		stringBuilder.Append("- Тест 8: Створення файлу останнього чека з бази даних");
		TestResult.Text = stringBuilder.ToString();
		Application.DoEvents();
		if (SaveLastCheck())
		{
			stringBuilder.Append("\r\n");
			stringBuilder.Append("- ОК!");
			TestResult.Text = stringBuilder.ToString();
		}
		else
		{
			stringBuilder.Append("\r\n");
			stringBuilder.Append("- ERROR!");
			TestResult.Text = stringBuilder.ToString();
		}
		Application.DoEvents();
		((Control)OkB).Enabled = true;
		((Control)NoB).Enabled = false;
		StopTest = false;
		stringBuilder.Append("\r\n");
		stringBuilder.Append("- Перевірка виконана!");
		TestResult.Text = stringBuilder.ToString();
		Application.DoEvents();
		if (CloseTest)
		{
			((Form)this).Close();
		}
	}

	private string LastErrorN()
	{
		string text = All.f.StringGetFn(All.A.FN, "LastOfflineErr").Trim();
		if (text.Length < 43)
		{
			return "";
		}
		return Conversions.ToString(text[40]) + Conversions.ToString(text[41]) + Conversions.ToString(text[42]);
	}

	private void РозпочатиТестуванняToolStripMenuItem_Click(object sender, EventArgs e)
	{
		TestStart();
	}

	private void ОновитиСертифікатиToolStripMenuItem_Click(object sender, EventArgs e)
	{
		CloseTest = false;
		МенюToolStripMenuItem.Enabled = false;
		((Control)OkB).Enabled = false;
		SertUpLoad();
		МенюToolStripMenuItem.Enabled = true;
		((Control)OkB).Enabled = true;
		CloseTest = true;
	}

	private void SertUpLoad()
	{
		string text = "http://lic.webchek.com.ua/prro-tax-gov-ua-chain.lic";
		string text2 = All.MyDoc() + "\\WebCheck\\prro-tax-gov-ua-chain.tmp";
		string text3 = All.MyDoc() + "\\WebCheck\\prro-tax-gov-ua-chain.pem";
		string text4 = "prro-tax-gov-ua-chain.pem";
		string text5 = "http://lic.webchek.com.ua/CACertificates.lic";
		string text6 = All.MyDoc() + "\\WebCheck\\keys\\CACertificates.tmp";
		string text7 = All.MyDoc() + "\\WebCheck\\keys\\CACertificates.p7b";
		string text8 = "CACertificates.p7b";
		StringBuilder stringBuilder = new StringBuilder();
		TestResult.Text = "";
		TestResult.Text = stringBuilder.ToString();
		Application.DoEvents();
		stringBuilder.Append("- Запущено процедуру оновлення сертифікатів!");
		stringBuilder.Append("\r\n");
		stringBuilder.Append("- Завантаження сертифікатів із сервера...");
		TestResult.Text = stringBuilder.ToString();
		Application.DoEvents();
		if (File.Exists(text2))
		{
			FileSystem.DeleteFile(text2);
		}
		if (File.Exists(text6))
		{
			FileSystem.DeleteFile(text6);
		}
		try
		{
			((ServerComputer)MyProject.Computer).Network.DownloadFile(text, text2);
			((ServerComputer)MyProject.Computer).Network.DownloadFile(text5, text6);
		}
		catch (Exception ex)
		{
			ProjectData.SetProjectError(ex);
			Exception ex2 = ex;
			stringBuilder.Append("\r\n");
			stringBuilder.Append("- Помилка: " + ex2.Message);
			TestResult.Text = stringBuilder.ToString();
			ProjectData.ClearProjectError();
			return;
		}
		stringBuilder.Append("\r\n");
		stringBuilder.Append("- ОК!");
		TestResult.Text = stringBuilder.ToString();
		Application.DoEvents();
		stringBuilder.Append("\r\n");
		stringBuilder.Append("- Перевірка файлів...");
		if (!File.Exists(text2))
		{
			stringBuilder.Append("\r\n");
			stringBuilder.Append("- Помилка");
			TestResult.Text = stringBuilder.ToString();
			return;
		}
		if (!File.Exists(text6))
		{
			stringBuilder.Append("\r\n");
			stringBuilder.Append("- Помилка");
			TestResult.Text = stringBuilder.ToString();
			return;
		}
		stringBuilder.Append("\r\n");
		stringBuilder.Append("- ОК!");
		TestResult.Text = stringBuilder.ToString();
		Application.DoEvents();
		if (File.Exists(text3))
		{
			FileSystem.DeleteFile(text3);
		}
		if (File.Exists(text7))
		{
			FileSystem.DeleteFile(text7);
		}
		FileSystem.RenameFile(text2, text4);
		FileSystem.RenameFile(text6, text8);
		stringBuilder.Append("\r\n");
		stringBuilder.Append("- Сертифікати оновлено успішно!");
		TestResult.Text = stringBuilder.ToString();
	}
}
